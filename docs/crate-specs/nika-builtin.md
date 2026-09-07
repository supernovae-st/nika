# Crate spec — `nika-builtin`

| | |
|---|---|
| Status | **SPEC** (Gate 1 · authored 2026-06-11 · announce-ladder step s16) |
| Layer | **L1.5** — the builtin tool layer · above the L1 effects it composes · below the L2 verbs that dispatch into it |
| Design | the 24 canonical stdlib builtins behind ONE dispatcher implementing the three kernel tool seams (`ToolExecuteDyn` + `ToolBatchDyn` + `ToolDefinitionProviderDyn`) |
| Normative source | `nika-spec stdlib/builtins-v0.1.md` (the 24 · contracts · error codes) + `stdlib/extract-modes-v0.1.md` (fetch modes) + `spec/05-errors.md` (4-segment code grammar) — **this doc never restates a contract, it cites** |
| LOC budget | ≤15k crate · ≤1500/file · ≤100/fn (Diamond caps) — one module per builtin family |
| Crate version | tracks workspace |
| License | `AGPL-3.0-or-later` |
| Publish | `false` — internal L1.5 |
| NIKA codes | spec-form 4-segment strings (`NIKA-BUILTIN-<NAME>-NNN`) carried in `ToolResult.content` — see §3 |

## §1 · Purpose

The real tool layer. `nika-verb-invoke` and `nika-verb-agent` dispatch over
the kernel `ToolExecuteDyn` seam, and the agent enumerates definitions over
`ToolDefinitionProviderDyn` — until now only mocks implement either. This
crate is the production implementation: a **closed registry of the 24
stdlib v0.1 builtins** (core 6 · file 5 · data 8 · introspection 2 ·
network 2 · media 1), each a thin composition over kernel effect seams, plus the
model-facing `ToolDef` (name · description · JSON-Schema params) for every
tool.

## §2 · Architecture — the dispatcher and its seams

```text
                    ┌───────────────────────────────────────┐
 verbs (L2) ──────▶ │ BuiltinDispatcher<F, H, C, E, P, W>   │  implements
   invoke · agent   │   the closed 24-registry              │  ToolExecuteDyn
                    │   route(name) → the builtin fn        │  ToolBatchDyn
 agent tool-defs ─▶ │   tool_defs() → 24 × ToolDef          │  ToolDefinitionProviderDyn
                    └──┬────┬────┬────┬─────┬────┬──────────┘
                       │    │    │    │     │    │
                  F: Fs │ H: HttpClient │ C: ClockDyn │ E: Emitter
                       │                │
                  P: Prompter (LOCAL)   W: WorkflowIntrospect (LOCAL)
                       │
                  image plane (OPTIONAL) · image_http: Option<Arc<H>> +
                  ImageKeys · with_image_plane() at the composition root
                  (the PROVIDER client · SSRF-disabled const endpoints ·
                  600s ceiling · ADR-105) — unwired ⇒ mock-only, real
                  providers fail the precise -002
```

- **Kernel seams consumed** (all `trait_variant` Dyn · generics not
  `Box<dyn>` per house pattern): `FsReadDyn+FsWriteDyn+FsListDyn` (file 5)
  · `HttpGetDyn+HttpPostDyn` (fetch · notify) · `ClockDyn` (wait · date
  `op:now`). The event seam `Emitter` (log · emit) is **LOCAL** — owned here
  (single-consumer · alongside `Prompter`/`WorkflowIntrospect` below), NOT a
  kernel trait (the spec earlier mislabelled it `EventSinkDyn`).
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
| log · emit | `Emitter` | best-effort (log never fails) · emit shape-gates `event_type` regex → `NIKA-BUILTIN-EMIT-001` |
| assert | — | `condition` arrives CEL-resolved (a boolean) · false → `NIKA-BUILTIN-ASSERT-001` |
| prompt | `Prompter` | 3 modes · non-interactive contract normative (stdlib §prompt) · `choice` validates `default:`∈`choices:` EAGERLY (PROMPT-002 is a parse-error class — it fires even when a human would have answered) |
| done | — | reject `NIKA-BUILTIN-DONE-001` |
| wait | `ClockDyn` (`sleep` + `system_now`) | duration XOR until · Go-duration parse · ABSOLUTE `until:` shipped (wall-clock compare via the injected clock · `timeout:` cap honored) · `-001` input/timeout · `-002` past timestamp · `-003` exactly-one-of |
| read · write · edit · glob · grep | `Fs*Dyn` | text/binary read · overwrite/create_dirs · literal find/replace-all (RMW not atomic across concurrent edits — DAG ordering serializes) · sorted glob (exclude filter = iterative DP matcher · polynomial on adversarial patterns) · `(path,line)`-sorted grep over `regex` crate (RE2-class) · grep skips unreadable entries deliberately (dirs/binary/raced · `grep -rs` semantics · spec allocates only `-001`) |
| jq | `jaq` 3.1 (jaq-core+std+json · MIT · API source-verified 2026-06-11) | **exactly-one-output** (the 04-variables.md:347 binding law applied to the tool) · 0/N outputs → `NIKA-BUILTIN-JQ-001` advising `[…]` · compile errors are also `-001` at runtime (static catch is `NIKA-VAR-005`, the check ladder's job) · rendered output ≤16 MiB per value · non-finite results named actionably · runs on `spawn_blocking` (sync CPU off the executor) |
| json_diff | `json-patch` 4.2 (MIT/Apache) | RFC 6902 `diff()` |
| json_merge_patch | `json-patch` 4.2 | RFC 7396 `merge()` — null-deletes (jq's `*` can't) |
| validate | `jsonschema` (already workspace) + `serde_yaml_bw` | report-never-fail `{valid, errors}` · `-001` bad schema · `-002` yaml parse |
| convert | `toml` 1.1 + `csv` 1.4 + `serde_yaml_bw` 2.5 + `serde_json` | 4 formats · 12 directions · `from==to` → `-001` · parse fail → `-002` (spec names these reference crates) · TOML datetimes bridge to ISO 8601 strings (typed walk — never the serde `$__toml_private_datetime` sentinel) · TOML non-finite floats → `-001` (not JSON-representable) · a nested object value in a CSV row emits as its compact-JSON cell |
| uuid | `uuid` (workspace) | v7 default / v4 · tests pin FORMAT + version nibble (not value) |
| date | `jiff` 0.2 (Unlicense OR MIT · bundles IANA tzdb — sovereign, zero system dependency) + `ClockDyn` | the spec's FULL six ops (now · add · subtract · format · parse · diff) · `op:now` rides `ClockDyn::system_now` (hermetic under MockClock) + IANA `tz:` · format/parse speak strftime · diff returns an integer in `unit:` (seconds default · ms/min/h/days) · `-001` |
| hash | `blake3` + `sha2` (both workspace) | blake3 default · md5/sha1 → `-001` |
| fetch | `HttpGetDyn`/`HttpPostDyn` + `nika-extract` (8 modes) + `data::jq` (mode jq) | non-2xx → `-001` with `BuiltinFailure.transient` per the normative status table (5xx/408/429 true · other 4xx false) · transport timeouts/connection failures transient too · SSRF lives in the L1 http effect (3-layer · s5 · verified) — this layer does NOT re-implement it · **the `mode:` surface is WIRED (step 13)**: 8 modes via `nika-extract` (default markdown) · `mode: jq` composes `data::jq` (the one jq engine · one-output law reused not re-implemented) · `raw`/`jq` strict-UTF-8 (non-UTF-8 → `-001` per spec raw contract) · extraction modes charset-aware decode from `Content-Type` (encoding_rs) · the whole parse runs on `spawn_blocking` (a 64 MiB HTML parse must not starve the executor · the `data::jq` precedent) |
| notify | `HttpPostDyn` | `webhook` MUST · other channels `-001` unconfigured · non-2xx `-002` carries `transient` per the same status table |
| inspect | `WorkflowIntrospect` | 4 views · `-001` unknown view |
| image_generate | the image plane (`HttpPostDyn` · OPTIONAL) + `Fs*Dyn` + `ClockDyn` + `Emitter` + `FsBoundary` | stdlib §Media (ADR-105) · module family `src/image/` (args · sniff · mock · save · manifest · embed · openai · gemini · xai · local) · local compat wire (sovereign · engine-config base URL) / openai `gpt-image-2` native `n` / gemini `gemini-3.1-flash-image` n-sequential / xai `grok-imagine-image` aspect+resolution classes / deterministic mock (hand-rolled stored-deflate PNG · validated by the independent `png` dev-dep) · header-only decode validation (magic authority · no pixel decode) · boundary-gated atomic saves + collision probing + idempotent re-runs · `manifest_version: 1` provenance + in-PNG `nika` tEXt chunk (deterministic · survives cp) · exact `cost_usd` (xai ticks) metered into the run ledger · base64 never rides outputs (`debug:` echo sanitized) · keys = composition-root `ImageKeys` (env ladder · zeroizing) · codes `-001..-007` |

### Honest gaps (delegations, not omissions)

`write overwrite:false` retains the stdlib §write arguments, path result and
WRITE-002 code, but routes publication through `FsWriteDyn::write_new` so a
target created after the early absence check remains intact. `JudgedFs`
delegates this operation after the existing dispatch guard and parent policy.
Backends without exclusive publication refuse through WRITE-001; implement
the provided kernel method to migrate them. Replacement writes and edit retain
their existing seam. Publication is neither fsync durability nor proof that a
cancelled blocking operation has stopped; inspect the actual file before retry
after an uncertain result.

- **Path capability gating** (`nika:write` outside-CWD rejection · NIKA-204
  class): NOT here — `nika-policy` (L1.5 · design locked, impl gated on
  kernel-migration) sits between the verbs and this dispatcher. A canary
  test pins today's pass-through so the policy landing flips it visibly.
- **jq evaluation cost**: the 16 MiB ceiling bounds the RENDERED output;
  jaq's internal materialization (`[range(1e9)]` builds in-engine before
  any output is yielded) is bounded by the engine's task-level supervision
  (timeout · memory caps), not re-implemented per-builtin. `spawn_blocking`
  keeps the executor responsive meanwhile.
- **`BuiltinFailure.transient`** is typed at the failure plane; the wire
  `ToolResult` has no metadata slot yet — the flag projects when the kernel
  grows one (both types `#[non_exhaustive]`, strictly additive).

## §5 · Testing strategy

Mock-first over kernel-mock (`MockFs` · `MockHttp` · `MockClock` ·
`NullEventSink`) + local mocks for the two owned seams. Per-builtin unit
tests pin the spec contract lines (codes · defaults · sort orders ·
exactly-one-output). Dispatcher tests pin: routing totality (all 24
addressable · unknown → NotFound) · `tool_defs()` returns 24 schemas ·
done rejected · batch = sequential map. Property: jq exactly-one-output
over arbitrary JSON · glob/grep determinism · sniff totality on arbitrary
bytes (+ magic-prefixed tails) · sanitize_component traversal-freedom.
Image adapters: MockHttp wire fixtures (request-shape pins · error-plane
tables · the-key-never-leaks sweeps) · mock PNG files decoded by the
independent `png` dev-dep. Mutation ≥90%.

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

## §8 · The 12 gates (readiness map)

| Gate | Status |
|---|---|
| 1 SPEC | ✅ this file + ADR-096 (the 23rd builtin `nika:compose`) + ADR-105 (the 24th · `nika:image_generate` · stdlib §Media graduate) + the 25th (`nika:tts_generate` · stdlib §Audio) + the normative `nika-spec stdlib/builtins-v0.1.md` (25 builtins) |
| 2 TDD | ✅ RED→GREEN · mock-first over `nika-kernel-mock` (`MockFs` · `MockHttp` · `MockClock` · `NullEmitter`) + the two local-seam mocks · per-builtin contract tests + dispatcher routing-totality (all 25 addressable · unknown → `NotFound` · `nika:done` rejected) |
| 3 IMPL | ✅ `#![forbid(unsafe_code)]` · **0 `.unwrap()`/`.expect()` in `src/`** (`?` + `unwrap_or`/`ok_or` · `BuiltinFailure` total) |
| 4 CLIPPY 0 | ✅ `cargo clippy -p nika-builtin --all-targets -- -D warnings` = 0 |
| 5 MUTATION ≥90 | ✅ **FLOOR 91.3%** — `cargo-mutants -p nika-builtin` clean low-load run (2026-06-16): **359 caught / 393 viable** (22 `missed` · 12 `timeout` · 64 unviable excluded · `caught/viable` ≥ 90 → FLOOR mode, no exemption). The killable survivors are dead post-hardening (the jq error-render fns + the permits access-category projection assert message CONTENT · `missed` 30→22). The residual ~34 are **equivalent mutants** (base64 disjoint-bit `\|`↔`^` · `\|`==`^` on disjoint bits), **hard byte-scanner edges** (charset detection · `is_not_a_directory` non-ENOTDIR), **permits defensive guards** (`parent != cur` · `unbounded()`↔`Default` — never diverge for a real path), and **I/O-bound `timeout` mutants** the harness can't distinguish from real hangs — **none an exploitable bypass** (rust-security verdict **SOUND** on file/net/permits · the `request.method` mapping IS pinned). The earlier 86.8% was contention-inflated timeouts (31 under concurrent load); this low-load run is the true FLOOR. |
| 6 PROPTEST | ✅ jq exactly-one-output over arbitrary JSON · glob/grep determinism (naive-equivalence) · base64 round-trip · `parse_go_duration` totality |
| 7 BENCH | ➖ N/A — a thin composition over the L1 effect seams · the hot paths (fs walk · http · jq materialization) are bounded in their owning layers (exemption per ADR-003 « if applicable ») |
| 8 DOC 0 | ✅ `RUSTDOCFLAGS="-D warnings" cargo doc -p nika-builtin --no-deps` = 0 |
| 9 CANARY E2E | ✅ the builtins dispatch through the real `BuiltinDispatcher<TokioFs, …>` (`boundary_dispatch_tests` · real fs) + a live `nika run` exercising `invoke nika:*` over the shipped L3 runtime |
| 10 GOLDEN PARITY | ➖ N/A — greenfield Diamond rewrite · no legacy dispatcher to match (CRAFT not extraction · ADR-001) |
| 11 REVIEW SWARM | ✅ 3 parallel sliced reviewers — **rust-security** (file + net + permits · VERDICT **SOUND** · no bypass · the runtime fs boundary is structurally unbypassable · SSRF correctly delegated to L1) · **feature-dev** (the 8 data builtins · correct · 1 message fix) · **spn-nika** (dispatcher + conventions). Convergent findings RESOLVED: the 22↔23 count drift · the model-facing `nika:compose` whitespace · the `EventSinkDyn`→`Emitter` spec mislabel · the `json_merge_patch` RFC-subset message. |
| 12 ATOMIC COMMIT | ⏳ this admission commit removes `nika-builtin` from `workspace.metadata.diamond.wip`. |

🦋 Nika — workflow engine for AI, AGPL, SuperNovae Studio.
