# Historical transform records

Pending and verified transform records written by the `fcf290a7b` compiler (the reader before
the saved-plan strictness work), byte-for-byte as the independent E14 review produced them on
2026-09-28 through a harness pinned to that source (`e14-f7-old`). They are evidence of what a
persisted record looks like in the wild, so they are never regenerated: a reader that cannot
replay them has changed an identity, not the record.

| file | intent | shape |
|---|---|---|
| `f7-pending.json` | shared domains after an active-status filter | a single-clause rule, a compute detail joined with « ; » |
| `f7-verified.json` | the same, field answered | the same plus the verified program |
| `para-pending.json` | shared domains | a compute detail the seat paraphrased |
| `para-verified.json` | the same, field answered | the same plus the verified program |

The observations are synthetic (`peek_sha256: e14-synthetic-observation`); no path is private.
