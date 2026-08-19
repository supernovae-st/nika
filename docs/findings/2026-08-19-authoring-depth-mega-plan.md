# Authoring depth · mega-plan (2026-08-19)

The authoring loop is the product. A file that only *parses* is the
floor. The next agent who is asked to write Nika should land a file
that is **clean, compiled, and paid_ready** — four verbs, `${{ }}`
CEL, nine-key envelope, jq/decide as the law.

**Papers this train already used** · Huang 2310.01798 · LLM-Modulo
2402.01817 · MIPRO 2406.11695 · AlphaCodium 2401.08500 · RLM
2512.24601 (file = environment · `for_each`/`invoke` = recursion ·
jq/decide = verification). No 5th verb. No critic infer.

**Not this train** · NEP-0021 · `v0.111.0` tag from a feature
branch · folding hints into `is_clean` · `nika:compile` · a scoring
DSL · making `--native-strict` mean zero hints.

## Ladder (never fuse the rungs)

| Rung | Field | Means | Who reads it |
|---|---|---|---|
| 1 legal | `clean` / `is_clean` | parses · permits · DAG | `nika check` exit |
| 2 compiled | no `unproven-law` | every infer→jq/decide law has a const-fixture `nika:assert` | `paid_blockers` today · `compiled` stamp Wave 1 |
| 3 paid | `paid_ready` | no paid-run footguns | `nika check --json` · `nika explain` |
| 4 MCP fail | only `infer-as-law` + `digit-string-enum` | the expensive pair | `nika_check` oracle |

`paid_ready` already contains rung 2 (`unproven-law` ∈ `PAID_RUN_KINDS`).
Rungs stay distinct so an agent can say "the file is legal but the law
is unproven" without lying that it is unclean.

## Already shipped (this arc, on `feat/authoring-seams-followup-2026-08-19`)

- `infer-as-law` (phrase list + never-clause · locale/sentiment silent)
- `paid_ready` / `paid_blockers` on the JSON report
- leftover door lift (`11-lift` · `invoke: nika:fetch` · `config.*` coffined)
- **this commit** · `unproven-law` · lesson 13 prove+assert · lesson 14
  `nika:decide` (OWED `decide` struck) · public-api snapshot for the
  paid_* surface · spec `llms-full.txt` projection

Marketplace kit (`nika-plugins`) follows **released** `v0.110.0`.
Teaching in the mirrored `SKILL.md` lands with the next brew tag via
`release-heal`, not this PR. Docs and engine pack teach now.

## Wave 1 · machine-readable next (this follow-up)

Small. Same four verbs. No new builtin.

1. **Stamp `compiled`** next to `paid_ready` on the JSON report
   (`compiled: true` iff no `unproven-law` hint). Additive.
   `report_version` stays 1. `is_clean` untouched.
2. **Stamp `next`** when `paid_ready` is false: the first paid
   blocker `{kind, task, advice}`. Agents stop parsing the hint
   array to decide what to fix.
3. **MCP `nika_check`** returns `compiled` + `next`. Hard-fail set
   unchanged (infer-as-law + digit-string-enum only).
4. Tests · unproven file ⇒ `compiled: false` + `next.kind ==
   "unproven-law"` · lesson 13 ⇒ `compiled: true`.

Refuse · promoting `unproven-law` to a finding · growing `LAW_PHRASES`.

## Wave 2 · leftover dialect (SHIPPED this wave)

Grep-and-kill on **teaching** surfaces (pack · spec examples ·
guides · SKILL · QUICKSTART). Not a grammar change. Live leftover
was `ROADMAP.md` teaching `nika.toml` + `{{...}}` · now `nika.yaml`
+ `${{ }}`. Detector tests may keep a dead form as the input that
must hint. Prose and runnable examples must not teach it.

| Dead form | Live form |
|---|---|
| `nika.toml` as the project file | `nika.yaml` |
| `that:` on `nika:assert` | `condition:` + `message:` |
| `fetch:` as a verb | `invoke: nika:fetch` |
| `nika: v1` + `workflow:` | nine-key `nika: <kebab-id>` |
| 5-verb lists | 4 verbs · fetch is a tool |
| `config:` / `vars:` / `env:` envelope | `inputs:` / `const:` / `secrets:` |

## Wave 3 · compose lesson (SHIPPED this wave)

`nika:compose` is **loop-served only** (standalone `invoke` →
`NIKA-BUILTIN-COMPOSE-001`). Lesson
`15-compose-self-check.nika.yaml` is an `agent:` whitelist
(`nika:done` first, then `nika:compose`) that drafts a nine-key
hello and iterates on the check JSON until `valid`. Mock/echo
closes at turn one. OWED `compose` struck.

Do **not** fake it with a standalone invoke. Do **not** execute the
draft inside the same run ("generation is not permission").

## Wave 4 · inspect (SHIPPED this wave)

`LiveInspect` is injected at composition (same `Arc` the dispatcher
holds). The runtime seeds the DAG at run start and mirrors records +
spend after each wave. Hint `inspect-unwired` retired. Lesson
`16-inspect-self` asserts `available`. OWED `inspect` struck.
`NoWorkflow` stays for isolated dispatcher tests.

## Wave 5 · still open, not this authoring train

| Item | Why later |
|---|---|
| per-iteration `for_each` resume | runtime · ~15k wall |
| `tts_generate` showcase | audio graduate · not a logic gap |
| NEP-0021 | still gated |
| tag `v0.111.0` | release train after merge to `main` |

## RLM mapping (why this order)

| RLM (2512.24601) | Nika already | What's missing |
|---|---|---|
| file = environment | the `.nika.yaml` *is* the scratchpad | agents write once and hope |
| recursion | `for_each` · `invoke: { workflow: }` · lesson 15 `nika:compose` · lesson 16 `nika:inspect` | per-iteration resume (ADR-099) |
| verification | jq / decide / assert · `compiled`/`next` | agents still write once and hope — the loop is the product |

The loop we want: write → `nika check --json` → if `next`, repair
that one task → re-check → handoff only when `paid_ready`.

## Verify

```
cargo test -p nika-check --lib -- hints
cargo test -p nika-cli --lib -- every_builtin
cargo test -p nika-pack --test pack_integrity
cargo clippy -p nika-check --all-targets -- -D warnings
python3 scripts/estate.py --write
# spec
python3 scripts/showcase-projector.py --check
python3 scripts/llms-projector.py --check
```
