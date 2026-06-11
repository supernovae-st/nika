# Crate spec — `nika-builtin`

| | |
|---|---|
| Status | **SPEC** (Gate 1 · authored 2026-06-11 · announce-ladder step s16) |
| Layer | **L1.5** — the builtin tool layer · above the L1 effects it composes · below the L2 verbs that dispatch into it |
| Design | the 22 canonical stdlib builtins behind ONE dispatcher implementing the three kernel tool seams (`ToolExecuteDyn` + `ToolBatchDyn` + `ToolDefinitionProviderDyn`) |
| Normative source | `nika-spec stdlib/builtins-v0.1.md` (the 22 · contracts · error codes) + `stdlib/extract-modes-v0.1.md` (fetch modes) + `spec/05-errors.md` (4-segment code grammar) — **this doc never restates a contract, it cites** |
| LOC budget | ≤15k crate · ≤1500/file · ≤100/fn (Diamond caps) — one module per builtin family |
| Crate version | tracks workspace (`0.80.0`) |
| License | `AGPL-3.0-or-later` |
| Publish | `false` — internal L1.5 |
| NIKA codes | spec-form 4-segment strings (`NIKA-BUILTIN-<NAME>-NNN`) carried in `ToolResult.content` — see §3 |

## §1 · Purpose

The real tool layer. `nika-verb-invoke` and `nika-verb-agent` dispatch over
the kernel `ToolExecuteDyn` seam, and the agent enumerates definitions over
`ToolDefinitionProviderDyn` — until now only mocks implement either. This
crate is the production implementation: a **closed registry of the 22
stdlib v0.1 builtins** (core 6 · file 5 · data 8 · introspection 1 ·
network 2), each a thin composition over kernel effect seams, plus the
model-facing `ToolDef` (name · description · JSON-Schema params) for every
tool.

## §2 · Architecture — the dispatcher and its seams

```text
                    ┌───────────────────────────────────────┐
 verbs (L2) ──────▶ │ BuiltinDispatcher<F, H, C, E, P, W>   │  implements
   invoke · agent   │   the closed 22-registry              │  ToolExecuteDyn
                    │   route(name) → the builtin fn        │  ToolBatchDyn
 agent tool-defs ─▶ │   tool_defs() → 22 × ToolDef          │  ToolDefinitionProviderDyn
                    └──┬────┬────┬────┬─────┬────┬──────────┘
                       │    │    │    │     │    │
                  F: Fs │ H: HttpClient │ C: ClockDyn │ E: EventSinkDyn
                       │                │
                  P: Prompter (LOCAL)   W: WorkflowIntrospect (LOCAL)
```

- **Kernel seams consumed** (all `trait_variant` Dyn · generics not
  `Box<dyn>` per house pattern): `FsReadDyn+FsWriteDyn+FsListDyn` (file 5)
  · `HttpGetDyn+HttpPostDyn` (fetch · notify) · `ClockDyn` (wait · date
  `op:now`) · `EventSinkDyn` (log · emit).
- **Local seams owned here** (single-consumer traits live with their
  consumer, not in the kernel — the `Prompter` has exactly one call site):
  - `Prompter` — answers `nika:prompt`. Ships `NonInteractive` (the
    normative CI contract: use `default:` else
    `NIKA-BUILTIN-PROMPT-001`). The L4 CLI implements the TTY prompter
    (L4→L1.5 = legal downward dep).
  - `WorkflowIntrospect` — answers `nika:inspect`'s 4 views from live
    run state. The L3 engine implements it per-run; tests mock it.
- **`nika:done` is REJECTED here** — the sentinel is agent-loop-owned
  (`NIKA-BUILTIN-DONE-001` per spec 05 — valid only inside an `agent:`
  whitelist; the loop intercepts it before any dispatcher exists).

## §3 · Error model — spec codes ride `ToolResult`

Two failure planes, matching the invoke-verb semantics shipped at s11:

1. **The tool RAN and failed** → `ToolResult { is_error: true, content:
   "NIKA-BUILTIN-<NAME>-NNN · <message>" }` — the spec's 4-segment string
   codes verbatim (grammar `spec/05-errors.md:47`), greppable, and exactly
   what the agent loop feeds back to the model. The spec table is
   normative for codes; this crate never invents one.
2. **The tool could not be addressed** (unknown `nika:` name) →
   `ToolExecError::NotFound` — the verb layer maps it to NIKA-450 with
   the did-you-mean already shipped in `nika check`.

Success values: every builtin returns a JSON-serializable value
(stdlib §cross-builtin invariants) serialized into `ToolResult.content`
(`null` prints as `null` · strings carry verbatim · structured values as
compact JSON). One rendering, one seam.

## §4 · Per-builtin implementation notes (contracts cited, not restated)

| Builtin | Composes | Notes |
|---|---|---|
| log · emit | `EventSinkDyn` | best-effort (log never fails) · emit shape-gates `event_type` regex → `NIKA-BUILTIN-EMIT-001` |
| assert | — | `condition` arrives CEL-resolved (a boolean) · false → `NIKA-BUILTIN-ASSERT-001` |
| prompt | `Prompter` | 3 modes · non-interactive contract normative (stdlib §prompt) |
| done | — | reject `NIKA-BUILTIN-DONE-001` |
| wait | `ClockDyn` | duration XOR until · Go-duration parse · `-001/-002/-003` |
| read · write · edit · glob · grep | `Fs*Dyn` | text/binary read · overwrite/create_dirs · literal find/replace-all · sorted glob · `(path,line)`-sorted grep over `regex` crate (RE2-class) |
| jq | `jaq` 3.1 (jaq-core+std+json · MIT · API source-verified 2026-06-11) | **exactly-one-output** (the 04-variables.md:347 binding law applied to the tool) · 0/N outputs → `NIKA-BUILTIN-JQ-001` advising `[…]` · compile errors are also `-001` at runtime (static catch is `NIKA-VAR-005`, the check ladder's job) |
| json_diff | `json-patch` 4.2 (MIT/Apache) | RFC 6902 `diff()` |
| json_merge_patch | `json-patch` 4.2 | RFC 7396 `merge()` — null-deletes (jq's `*` can't) |
| validate | `jsonschema` (already workspace) + `serde_yaml_bw` | report-never-fail `{valid, errors}` · `-001` bad schema · `-002` yaml parse |
| convert | `toml` 1.1 + `csv` 1.4 + `serde_yaml_bw` 2.5 + `serde_json` | 4 formats · 12 directions · `from==to` → `-001` · parse fail → `-002` (spec names these reference crates) |
| uuid | `uuid` (workspace) | v7 default / v4 · tests pin FORMAT + version nibble (not value) |
| date | `jiff` 0.2 (Unlicense OR MIT · bundles IANA tzdb — sovereign, zero system dependency) + `ClockDyn` for `op:now` | op-discriminated · strftime grammar · `-001` |
| hash | `blake3` + `sha2` (both workspace) | blake3 default · md5/sha1 → `-001` |
| fetch | `HttpGetDyn`/`HttpPostDyn` + extract modes | non-2xx → `-001` (`transient` per status) · SSRF lives in the L1 http effect (3-layer · s5) — this layer does NOT re-implement it · extract modes per `extract-modes-v0.1.md` (R1 scope decided against that doc at impl time · gaps documented here honestly) |
| notify | `HttpPostDyn` | `webhook` MUST · other channels `-001` unconfigured |
| inspect | `WorkflowIntrospect` | 4 views · `-001` unknown view |

## §5 · Testing strategy

Mock-first over kernel-mock (`MockFs` · `MockHttp` · `MockClock` ·
`NullEventSink`) + local mocks for the two owned seams. Per-builtin unit
tests pin the spec contract lines (codes · defaults · sort orders ·
exactly-one-output). Dispatcher tests pin: routing totality (all 22
addressable · unknown → NotFound) · `tool_defs()` returns 22 schemas ·
done rejected · batch = sequential map. Property: jq exactly-one-output
over arbitrary JSON · glob/grep determinism. Mutation ≥90%.

## §6 · Wiring pass (at admission)

`.gitignore` lift · workspace members + `layers.nika-builtin = "L1.5"` +
wip · deny tokio wrapper (dev-dep) + new-dep license rows already in
allowlist (MIT · MIT/Apache · Unlicense-OR-MIT picks MIT) · kernel hub
doc row unchanged (string-form codes live in the spec, not the numeric
registry) · public-api baseline + coverage floor.

## §7 · The seam this resolves

`ToolDefinitionProviderDyn` shipped 2026-06-11 with only a mock
implementation. `BuiltinDispatcher::tool_defs()` is its first production
impl: the catalog half of the agent's `nika:*` whitelist universe. The
MCP half (`mcp:server/*` via live `tools/list`) arrives with `nika-mcp`
(step 18) as a second implementor composed by the engine.

🦋 Nika — workflow engine for AI, AGPL, SuperNovae Studio.
