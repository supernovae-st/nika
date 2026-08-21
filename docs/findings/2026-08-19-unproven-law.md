# Unproven law · valid ≠ the law is right

**Date** · 2026-08-19 · follows `paid_ready` (18f18373b)
**Papers** · Huang 2310.01798 · LLM-Modulo 2402.01817 · MIPRO 2406.11695 ·
AlphaCodium 2401.08500 · RLM 2512.24601 (no 5th verb)
**Not** · NEP-0021 · critic infer · `is_clean` change · v0.111 tag

## Axes (three, never fused)

| Field | Means |
|---|---|
| `clean` / `is_clean` | parses · permits · DAG |
| `paid_ready` | no paid-run footguns (digits · glob README · inspect · jq-as-map · infer-as-law · **unproven-law**) |
| MCP hard-fail | only `infer-as-law` + `digit-string-enum` |

## Detector

A task is a **law** when it is `nika:jq` or `nika:decide` and it binds
an `infer:` output. The file is **compiled** when a jq/decide that does
*not* bind an infer is then `nika:assert`ed (`condition` reads `with.`).
Else each uncompiled law task gets hint `unproven-law`.

Never fail `is_clean`. Join `PAID_RUN_KINDS`.

## Teaching

- `13-extract-then-law` gains a const-fixture prove + assert (same jq).
- `14-decide-publish` is the owed `nika:decide` showcase (strike OWED).
- Same four verbs. jq/decide stay the law.

## Refuse

5th verb · LLM judge · `nika:compile` · folding this into `is_clean` ·
tagging from this branch.
