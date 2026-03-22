# DAG Patterns

15 workflows demonstrating every DAG pattern possible in Nika.
All use `exec:` and `fetch:` only — no API keys required.

## Basic Patterns (01-05)

| # | Pattern | Shape | Edges | Max Parallelism |
|---|---------|-------|-------|-----------------|
| 01 | Linear Chain | A -> B -> C -> D -> E | 4 | 1 |
| 02 | Diamond | A -> B+C+D -> E | 6 | 3 |
| 03 | Wide Parallel | A, B, C, D, E (independent) | 0 | 5 |
| 04 | Binary Tree | A -> B+C, B -> D+E, C -> F+G | 6 | 4 |
| 05 | Deep Chain + Bindings | A -> B(trim) -> C(upper) -> D(lower) -> E | 4 | 1 |

## Advanced Patterns (06-10)

| # | Pattern | Shape | Edges | Max Parallelism |
|---|---------|-------|-------|-----------------|
| 06 | Multi-Diamond | Two diamonds from shared source -> final merge | 10 | 4 |
| 07 | Conditional Fan-Out | Source -> 3 type-specific analyses -> report | 6 | 3 |
| 08 | Parallel Checks + Merge | 3 checks -> aggregate -> report | 6 | 3 |
| 09 | Cascade Transforms | trim -> upper -> lower -> base64 -> summary | 5 | 1 |
| 10 | Multi-Source Merge | 3 APIs -> combine -> analyze -> report | 5 | 3 |

## Real-World Patterns (11-15)

| # | Pattern | Shape | Edges | Max Parallelism |
|---|---------|-------|-------|-----------------|
| 11 | CI/CD Pipeline | checkout -> build+test+lint -> deploy -> notify | 7 | 3 |
| 12 | Data Warehouse ETL | extract(3) -> transform(3) -> load -> verify | 8 | 3 |
| 13 | Content Review | draft -> grammar+style+fact -> editor -> approved | 7 | 3 |
| 14 | Microservice Health | 5 checks -> aggregate -> alert | 7 | 5 |
| 15 | Research Synthesis | 3 searches -> for_each analyze -> synthesize -> report | 6 | 3 |

## Validate All

```bash
for f in *.nika.yaml; do nika check "$f"; done
```

## Run Any

```bash
nika run 02-diamond.nika.yaml
```
