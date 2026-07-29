# Crate spec — `nika-store`

| | |
|---|---|
| Status | **WIP — admission lane open** (F-P8 · lot 2 · NEP-0014-program §5) |
| Layer | L1 — memory satellite · the signed-write/verified-recall substrate · `Send + Sync` |
| Sub-tier | L1-deterministic — pure sign/verify + atomic file envelope · no async at the trait boundary |
| Design | **F-P8 (SMSR arXiv:2606.12703 · TMA-NM 2606.24322 · MemLineage 2605.14421)** — every store write is SIGNED at write, the label INSIDE the signature; every recall VERIFIES per entry — unsigned · bad signature · rewritten label = REJECTED (never filtered) |
| LOC budget | ≤1,020 src measured (entry ~220 · dir ~135 · sign ~310 · traits ~255 · errors ~50 · lib ~45) — re-allocated TWICE, honestly: the first sketch's ≤600 missed what the review swarm then priced (the filters and verdict classes the law requires: tenant fail-closed · level skip · name binding · version dispatch → ~820 estimated), and the implementation priced what the estimate still missed (the fold's failure-honesty walk · the third-leg rejection substrate · and the rustdoc the named-not-silent posture writes in full — no fake-compression to fit a number) |
| File cap | ≤1,500 LOC each |
| Function cap | ≤100 lines each |
| Crate version | tracks workspace |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 |
| Publish | `true` — publishable per ADR-004 |
| Reference | SMSR [arXiv:2606.12703](https://arxiv.org/abs/2606.12703) (no-provenance-free-filter theorem) · TMA-NM [arXiv:2606.24322](https://arxiv.org/abs/2606.24322) · MemLineage [arXiv:2605.14421](https://arxiv.org/abs/2605.14421) · ADR-030 (trust fail-safe) · ADR-031 (tenant scope) · ADR-078 (sealed memory traits) |

---

## 1. Purpose

`nika-store` is the **first honest writer of the engine's memory**: the
substrate's first slice of F-P8 (lot 2) — and, by construction, the
**TCB foundation of the Connectome's recall path**. The Connectome
(VISION_2040 · the `MemoryFrameRef.trust` note says it verbatim:
*"populate trust from real ingest provenance when the Connectome recall
path ships"*) can only ever be as trustworthy as the entries it
recalls; this crate is what makes an entry's provenance *verifiable*
before any orchestrator fuses, ranks, or feeds it to a model. The
substrate stays deliberately orchestrator-agnostic: whatever recall the
Connectome grows above it inherits verification for free.

A store entry carries its content
plus the provenance the SMSR theorem needs — store name · origin label ·
run id · timestamp · parents digest — and the whole envelope is **signed
at write** with the run-signing key (the engine = TCB · the LLM never
touches the key). At recall, every entry is verified **per entry**:

- **unsigned** → rejected (the no-provenance-free-filter theorem: an
  unsigned entry can never be filtered into trustworthiness — 0 %
  admission);
- **bad signature** (a byte flipped anywhere in the envelope — an entry
  written into the store OUTSIDE the engine, e.g. a direct file edit)
  → rejected;
- **label rewritten** → rejected-by-construction: the label rides INSIDE
  the signed preimage, so a relabel IS a signature mismatch.

Rejections are never silent filtering: each is a named verdict, a
journaled event (`memory_entry_rejected` · additive EventKind), and a
count the run's seal pins in `covers["memory"]` beside the admitted-set
digests — the receipt names the verified SET.

The crate implements the kernel memory traits
(`MemoryRemember · MemoryRecall · MemoryForget` + `Sealed` per ADR-078)
so the future L2 orchestrator (`nika-memory` · ADR-041 · W10) composes it
without knowing the signature exists.

## 2. Design decisions (locked · deviations named)

1. **Canonical bytes = JCS, not CBOR.** The law text mandates
   « CBOR-canonique »; the engine has zero CBOR substrate (ciborium is a
   transitive dev-dep only) and exactly ONE canonicalization voice:
   JCS, byte-pinned against the spec's Python reference via
   `nika_proof::preimage`. v1 signs the JCS of the envelope (minus the
   `sig` field). The deviation is named here; if the spec ever demands
   literal CBOR, that is a spec-repo amendment, not an engine dep.
2. **Label = `Integrity` (the live F-O1 lattice).** The runtime already
   computes each task's label (`task_integrity` · check≡run) and stamps
   it on the record — the write inherits it for free, and
   join = Untrusted-dominates is the live « max-des-arêtes ». The law's
   « lattice 4 niveaux » vs the engine's two label systems
   (`Integrity` 2-point · `TrustLevel` 5 constants) is a spec-side
   mapping question — named, v1 takes the live one.
3. **Key = the run-signing key, same custody** (OS keychain →
   `NIKA_RUN_KEY_FILE`/`NIKA_RUN_PUB_FILE` env → `~/.nika/keys/` 0600 —
   `load_signing_key` reused, never re-implemented). A dedicated memory
   keynum is the named v2 option; v1 signs with the same TCB the
   journal's seal already trusts.
4. **Store layout = `.nika/memory/<store>/`**, one file per entry
   (`<ulid-ish ts>-<digest-prefix>.json`), atomic temp+rename (the
   nika-fs pattern · readers never see a torn write).
5. **No YAML surface in v1** — `state.write[store]` syntax, a `memory:`
   schema key, and revived `nika:store`/`nika:recall` builtins are a
   spec amendment (v2). v1 is engine-API only, driven by tests.
6. **`HashDomain::Memory`** (a closed-set spec-15 amendment) rides the
   v2 owes; v1 signs the JCS bytes of the envelope directly.
7. **δ(t,m,k,n) hypergeometric certificate** → v1.5 (pure L0 math in
   nika-cap when it lands); the seal fold already carries the counts it
   will consume.
8. **The envelope is versioned (`"v": 1` INSIDE the signed preimage —
   FCI-003/011/022).** The signature commits to the format itself: every
   named v2 owe above changes the preimage, decode dispatches on `v`,
   and an unknown version rejects with the named
   `unsupported_version` (never a masquerading `bad_signature`). The seal
   fold carries its own `"v": 1` beside the store entries. Rejections
   are journaled one `memory_entry_rejected` event each BEFORE the seal
   (the law's third leg — the chain covers the names the fold counts),
   and a store whose walk fails rides the fold as a named
   `{"store", "error"}` entry, never a collapsed `None`.

## 3. Public API (draft — the only surface v1 admits)

```rust
//! `nika-store` · the signed-write / verified-recall memory substrate (F-P8).

/// One store entry — the signed envelope.
#[non_exhaustive]
pub struct StoreEntry {
    pub frame: serde_json::Value,     // the content (a MemoryFrame's value)
    pub label: nika_cap::Integrity,   // inherited at write · INSIDE the signature
    pub store: String,                // the named store
    pub run_id: String,               // the writing run
    pub ts_ms: u64,                   // write timestamp
    pub parents_digest: String,       // digest of the parent entry ids (the lineage)
    pub sig: String,                  // minisign box over the JCS of the above
}

/// The per-entry recall verdict (the sign.rs verdict shape).
#[non_exhaustive]
pub enum RecallVerdict {
    Admitted,
    Rejected(RejectReason), // Unsigned · BadSignature · LabelMismatch(≡ BadSignature by construction)
}

/// The deterministic, zero-LLM write path (F-F3): inherit the label,
/// sign the envelope, commit atomically.
pub fn remember_signed(dir: &Path, entry: UnsignedEntry, key: &minisign::SecretKey)
    -> Result<StoreEntry, StoreError>;

/// The verified recall: walk a store dir, verify every entry, admit or
/// reject — the caller journals the rejections.
pub fn recall_verified(dir: &Path, store: &str, pubkey: &minisign::PublicKey)
    -> Result<Vec<(StoreEntry, RecallVerdict)>, StoreError>;

/// The error surface (NIKA-605+ · one-voice registry).
#[non_exhaustive]
pub enum StoreError { /* Io · Serialize · KeyMismatch · DirLayout */ }

// + impls of the kernel traits: MemoryRemember · MemoryRecall · MemoryForget (+ Sealed)
```

## 4. Acceptance (the law's own pairs)

- **positive** — a legit write is signed + labeled; recall admits it;
  the seal's `covers["memory"]` pins the admitted digests + the
  rejection count.
- **negative 1** — an entry added OUTSIDE the engine (a direct file
  edit in the store dir) is REJECTED at recall (verify KO) — never
  admitted, never filtered.
- **negative 2** — a write derived from an untrusted recall that claims
  a Trusted label = signature mismatch = reject (the label inside the
  preimage is the only label that counts).
- **target** — an unsigned entry = 0 % admission (the SMSR theorem's
  floor).

## 5. Gates plan

1. SPEC — this file · 2. TDD — the four acceptance pairs red first ·
3. IMPL — this file's budget · 4. CLIPPY — 0 warnings · 5. MUTATION —
≥90 % killed (`cargo mutants -- --lib` — the acceptance/api/property
suites ride `src/tests/`, the prod-LOC-exempt dir-module convention, so
the lib-only gate exercises them; integration targets under `tests/`
never run there) · 6. PROPERTY — proptest envelope-tamper closure (any
byte flip ⇒ reject) · 7. BENCH — n/a (cold path) · 8. DOCS — 0
warnings · 9. CANARY E2E — `crates/nika-cli/tests/` on the
quarantine_e2e pattern (hermetic key via `NIKA_RUN_*_FILE`) ·
10. PARITY — n/a named (new substrate · no brouillon ancestor) ·
11. REVIEW SWARM — 3 agents · 12. ATOMIC COMMIT — `feat(nika-store):
admit to workspace — all 12 gates passed`.

## 6. v2 owes (named, never silent)

- `state.write[store]` YAML surface + revived builtins (spec amendment).
- `HashDomain::Memory` + the Python reference goldens (spec-15).
- The 4-vs-5 label-level lattice mapping (spec-side, F-F4 row).
- δ(t,m,k,n) hypergeometric certificate at check + receipt print (v1.5).
- Dedicated memory keynum + the `trust:` field population on
  `MemoryFrameRef` beyond the Integrity carry.
- The L2 `nika-memory` orchestrator (ADR-041 · type-state · BM25/RRF
  recall fusion) — v1 recall is list+filter. That orchestrator is the
  Connectome's recall path: when it ships, `MemoryFrameRef.trust`
  populates from this crate's verified provenance (the field is wired
  since ADR-030 · the writer is now here).
- `recall_verified`'s eager `Vec<EntryVerdict>` gains a
  streaming/iterator sibling when the L2 orchestrator lands (FCI-014 —
  the fold walks stores one at a time; a big store should not force a
  whole vec before the first verdict).
- `SignedMemoryStore::new`'s 7 positional parameters collapse into a
  builder when the surface grows (tenant id · anchor sets · a stateful
  clock) — v1 keeps the flat signature while the parameter set is
  exactly the trait surface's needs.
