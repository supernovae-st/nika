# Crate spec — `nika-rdfs-reasoner`

| | |
|---|---|
| Status | **Phase M proposal** (Gate 1 · shipped 2026-06-11 · admission at its Phase-M step of the Connectome climb (ADR-004 + ADR-042 lineage)) |
| Layer | L1 — reasoning satellite (M7) |
| LOC budget | ≤3,000 src |
| Crate version | tracks workspace |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 |
| Publish | `false` initially · **publishable crates.io standalone** post-admission (per ADR-004 · external ecosystem value) |
| NIKA codes | `NIKA-CNX-*` family · exact range allocated at ceremony via `nika_error::codes` registry (never invented here) |

---

## 1. Purpose

Deterministic inference over the graph — RDFS entailment + OWL 2 RL
materialization (subclass/subproperty transitivity · domain/range ·
inverse · transitive properties). Zero LLM. New triples land
provenance-tagged as `prov:wasDerivedFrom` the rule + premises.

## 2. Public API (sketch · Gate 1 level · final at ceremony)

```rust
pub struct Reasoner { /* ruleset */ }
impl Reasoner {
    pub fn materialize(&self, store: &mut ConnectomeStore) -> Result<DerivedCount, NikaError>;
    pub fn explain(&self, triple: &Triple) -> Option<DerivationChain>;
}
```

## 3. Dependencies (LOCKED · Connectome stack ratification 2026-06-11 · crates.io-verified)

`reasonable@0.4.4` (BSD-3 · OWL2-RL pure-Rust). horned-owl REJECTED (LGPL-3.0 · license wall).

## 4. SOTA references (ratified 2026-06-11 · arXiv-API-verified)

- W3C OWL 2 RL profile + RDFS entailment (specs · not arXiv)

## 5. Test strategy

W3C entailment test-suite subset as golden fixtures · derivation-chain explainability round-trip · idempotence (re-materialize = no new triples). Mutation ≥90%.

🦋 *Connectome satellite · « Memory is the function · the Connectome is the structure. »*
