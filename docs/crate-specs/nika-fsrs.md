# Crate spec — `nika-fsrs`

| | |
|---|---|
| Status | **Phase M proposal** (Gate 1 · shipped 2026-06-11 · admission at its Phase-M step of the Connectome climb (ADR-004 + ADR-042 lineage)) |
| Layer | L1 — forgetting satellite (M10) |
| LOC budget | ≤1,500 src |
| Crate version | tracks workspace |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 |
| Publish | `false` initially · **publishable crates.io standalone** post-admission (per ADR-004 · external ecosystem value) |
| NIKA codes | `NIKA-CNX-*` family · exact range allocated at ceremony via `nika_error::codes` registry (never invented here) |

---

## 1. Purpose

Optimal forgetting — FSRS (Free Spaced Repetition Scheduler · the
Anki 23.10+ algorithm) decides each record's retrievability and next
review · memories decay honestly instead of accumulating forever. The
recall pipeline weights results by retrievability; the consolidator
(M14 · future) uses due-review sets as its work queue. Augmented by the
Generative-Agents scoring triple (recency · importance · relevance).

## 2. Public API (sketch · Gate 1 level · final at ceremony)

```rust
pub struct MemoryScheduler { /* fsrs params */ }
impl MemoryScheduler {
    pub fn review(&mut self, id: RecordId, grade: Grade, at: Timestamp) -> NextReview;
    pub fn retrievability(&self, id: RecordId, at: Timestamp) -> f32;
    pub fn due(&self, at: Timestamp, limit: usize) -> Vec<RecordId>;
}
```

## 3. Dependencies (LOCKED · Connectome stack ratification 2026-06-11 · crates.io-verified)

`fsrs@6.6` (BSD-3 · open-spaced-repetition official · burn is dev-dep only · runtime = ndarray/rayon · pin `priority-queue@=2.7.0` noted).

## 4. SOTA references (ratified 2026-06-11 · arXiv-API-verified)

- FSRS · open-spaced-repetition (Anki 23.10+) · root model KDD 2022
- Generative Agents recency·importance·relevance · arXiv:2304.03442

## 5. Test strategy

Determinism fixtures (same review history → same schedule) · retrievability monotonic-decay property tests · due-set ordering. Mutation ≥90%.

🦋 *Connectome satellite · « Memory is the function · the Connectome is the structure. »*
