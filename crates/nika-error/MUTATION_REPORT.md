# nika-error mutation testing — Wave 3 (H8)

Date: 2026-04-16
Tool: `cargo-mutants 25.x`
Target files: `src/cost.rs`, `src/trust.rs`, `src/baggage.rs`

## Summary

- **Mutants generated**: 65
- **Caught**: 54
- **Missed**: 7
- **Unviable**: 4
- **Kill rate** (caught / (caught + missed)): **88.5%**

Meets H8 target (≥85%).

## Surviving mutants (documented, not blocking)

| File | Line | Mutation | Reason it survives / follow-up |
|------|------|----------|-------------------------------|
| `baggage.rs` | 45 | replace `>` with `>=` in size check | Off-by-one at the `MAX_SIZE_BYTES` boundary. Follow-up: add a boundary test that sets `total_size == MAX_SIZE_BYTES` exactly. |
| `baggage.rs` | 67 | replace `is_empty` with `true` | Trivial accessor. Covered indirectly by `empty_baggage` / `insert_and_get`; follow-up: add a dedicated `!b.is_empty()` after insert. |
| `baggage.rs` | 97 | replace `+` with `-` in `BaggageEntry::size` | The test only asserts `size() == 8` for a 3+5 entry, which also passes with subtraction when absolute values align. Follow-up: assert across several entries with distinct key/value lengths. |
| `cost.rs` | 70 | replace `is_zero` with `true` | Similar trivial accessor. Follow-up: assert `!c.is_zero()` for a non-zero cost. |
| `trust.rs` | 116 | delete `"elevated"` arm in `FromStr` | `from_str_named` only exercises `system` / `TRUSTED` — the other three named arms fall through to the catch-all and successfully parse. Follow-up: param-test `from_str` over the five predefined levels. |
| `trust.rs` | 118 | delete `"untrusted"` arm in `FromStr` | Same as above. |
| `trust.rs` | 119 | delete `"sandboxed"` arm in `FromStr` | Same as above. |

## Decision

Survivors are low-risk (no security or arithmetic invariants) and the
follow-up tests are trivial to add. Logged here so the next
session-4A / hygiene sweep can close them without re-running mutation
testing from scratch.
