# Error trait completeness · R4 audit table · 2026-06-10

> **B5 error one-voice close** (nika 10/10 arc · F8 + R4). Every
> workspace error enum × `{nika_code · is_transient · fingerprint}` ·
> grep-derived (Rule 7) · each row cites the test that pins it. The
> companion fix shipped same-arc · the M2 computer-use trio
> (`ScreenError` · `OcrError` · `A11yError`) spoke crate-local string
> `code()` accessors OUTSIDE the trait — migrated to `NikaErrorCode` +
> registry constants (`NIKA_1000..1206` · `Category::{Screen,Ocr,A11y}`).

## The one voice

`nika_error::traits::NikaErrorCode` is THE error-code surface ·
`nika_code() -> NikaCode` (registry constant) · `is_transient() -> bool`
(default `false` · override for retry-eligible classes) ·
`fingerprint() -> u64` (default hashes the code num · no overrides
workspace-wide — by design · finer grouping is a consumer concern).

## Implementing enums (13 production + 5 internal)

| Enum | Crate · home | nika_code range | is_transient | Pinning test |
|---|---|---|---|---|
| `CoreError` | nika-error `core_error.rs` | 001-049 Core | default false (all structural) | `core_error::tests::all_variants_not_transient` |
| `TransientError`/`OtherError`/`ChainedError` | nika-error `nika_error.rs` (internal wrappers) | passthrough | passthrough/true | `nika_error.rs` tests |
| `CatalogError` | nika-catalog `error.rs` | 010-015 Core/catalog | default false | `error.rs` range tests |
| `EventError` | nika-event `error.rs` | 420-422 Runtime | default false + explicit | `event_contract.rs` (420-422 pins) |
| `SchemaError` | nika-schema `error.rs` | 280-329 Schema (+ spec-facing `NIKA-<NS>-<NNN>` via `spec_code()` · orthogonal) | explicit (all static · false) | `spec_code.rs` `numbers_unique_within_namespace` (26) |
| `ShellError` | nika-kernel-core `errors.rs` | 050-099 Shell | default false | `shell_error_codes_in_range` |
| `BlobError` | nika-kernel-core `errors.rs` | 100-139 FileIo | default false | `blob_error_codes_in_range` |
| `HttpError` | nika-kernel-core `errors.rs` | 140-189 Http | default false | `http_error_codes_in_range` |
| `ProviderError` | nika-kernel-ai `errors.rs` | 330-379 Provider | explicit (RateLimited/Api-5xx true) | `provider_rate_limited_is_transient` |
| `MemoryError` | nika-kernel-ai `memory.rs` (co-located) | 601-604 Memory | explicit (Embedding/Storage true) | `memory_error_codes_in_range` + `fingerprint_differs_per_variant` |
| `ToolExecError` | nika-kernel-runtime `errors.rs` | 230-279 Mcp | default false | `tool_exec_error_codes_in_range` |
| `WasmPluginError` | nika-kernel-plugin `errors.rs` | 700 WasmPlugin (range-level) | default false | `wasm_plugin_error_code_in_range` |
| `SandboxError` | nika-kernel-plugin `errors.rs` | 750 Sandbox (range-level) | default false | `sandbox_error_code_in_range` |
| `ScreenError` ★ | nika-screen `error.rs` | **1000-1009 Screen** (B5 migration) | explicit (Capture/Init true) | `nika_codes_unique_in_range_and_registered` + `wire_codes_pinned` + `fingerprints_distinct_per_code` |
| `OcrError` ★ | nika-ocr `error.rs` | **1101-1109 Ocr** (B5) | explicit (Detect/Recognize/Join true) | `codes_are_unique_and_in_range` (registry-lookup asserted) |
| `A11yError` ★ | nika-a11y `error.rs` | **1201-1206 A11y** (B5) | explicit (Attribute/Walk/Join true) | `codes_are_unique_and_in_range` (registry-lookup asserted) |

★ = migrated this arc. Registry side · `nika_error::codes::ALL` carries
all 25 new constants · `lookup()` + `code_help()` resolve them ·
`computer_use_codes_have_their_categories` +
`computer_use_codes_lookup_and_help` pin the registry rows.

## Post-census additions (append-only · the census above stays frozen)

Crates admitted AFTER the 2026-06-10 census · same B5 one-voice contract
(`NikaErrorCode` impl · registry constants · pinned tests):

| Enum | Crate · home | nika_code range | is_transient | Pinning test |
|---|---|---|---|---|
| `VerbInferError` | nika-verb-infer `errors.rs` (s9) | 430-433 Verb | ProviderCall passthrough (RateLimited/5xx true) · rest false | `errors.rs` code/transient tests |
| `VerbExecError` | nika-verb-exec `errors.rs` (s10) | 440-442 Verb | Shell passthrough (kernel default false) · NonZeroExit false by design (a verdict, not transience) | `errors.rs` pair table |
| `VerbInvokeError` | nika-verb-invoke `errors.rs` (s11) | 450-452 Verb | Dispatch passthrough · rest false | `errors.rs` tests |
| `VerbAgentError` | nika-verb-agent `errors.rs` (s12) | 460-467 Verb | per-variant (budget/turn caps false) | `errors.rs` tests |
| `RuntimeError` | nika-runtime `errors.rs` (s18 · L3) | **1700-1703 Runtime** | never (contract breaches + static expression classes · retry never helps) | `unique-in-range` + `code-first Display` + `registry lookup` + `never-transient` |

Task-plane note · the runtime's `TaskErrorRecord` (spec 05 §error
structure) is a VALUE, not an enum — it carries whichever wire code the
failing verb/template produced (`code` + `message` + `transient`) into
`tasks.X.error` reads and `TaskFailed` event details. The spec-form
timeout class `NIKA-TIMEOUT-001` is stamped by the runtime at the task
timeout boundary (spec 03 · catchable · never retryable).

## Documented exemptions (not gaps)

| Enum | Why exempt |
|---|---|
| `CodegenError` (nika-catalog-codegen) | BUILD-TIME tool error (build.rs generator) · never crosses a runtime/verb boundary · thiserror Display suffices · zero NIKA range owed |
| `GoDurationError` · `ExprError` (nika-schema) | internal intermediate errors · wrapped into `SchemaError` before crossing the crate boundary · the wrapper carries the code |
| `ToolErrorPolicy` · `OnError` · `ErrorCategory` | NOT error types (policy/config enums matching the `*Error` name pattern) |
| `TestError`/`OtherError` (traits.rs tests) | test fixtures |
| `ExtractError` (nika-extract) | pure L1.5 transformation error · its only consumer (`nika-builtin` fetch) flattens every variant onto the spec-form `NIKA-BUILTIN-FETCH-001` string at the dispatcher boundary (crate spec) · the builtin plane carries the code |
| `SkillDefect` | nika-schema | wrapped-intermediate | the NIKA-AGENT-004 defect detail class — flattened onto the single spec-form finding at the checker/composer boundary; the wrapper carries the code (§skills) |
| `CelErrorKind` (nika-cel) §cel-subset | **spec-plane** carrier · the `cel-subset/0.1` failure class carries SPEC wire codes (`NIKA-VAR-001`/`005`/`006` · resolvable via `nika_pack::error_codes()`) NOT a `nika_error` registry range — same plane as the runtime's `NIKA-TIMEOUT-001`. The host maps it to `RuntimeError::{CelEval,UnresolvedTemplate,WhenUnsupported}` which carries the registry code (NIKA-1702/1703) at the runtime boundary. The struct `CelError` wraps it for Display; neither is an engine-internal enum owing a registry code |
| `CadenceErrorKind` (nika-cadence) §arm-registry | **spec-plane** carrier · the `arm:`-registry refusal class carries the grammar's OWN wire slugs (`cadence.*` via `spec_code()`) NOT a `nika_error` registry range — none owed: the L4 verb (`sign cadence`, V3⑪ of the arming plan) renders the taught fix at exit 2, the FILE plane. The struct `CadenceError` wraps it for Display; neither is an engine-internal enum owing a registry code. (The one-voice detector names the KIND enum — the thiserror `#[error(]` lives on the wrapper struct ~85 lines below, the awk proximity attribution the arming plan §2unvicies ⑤ notes.) |
| `CommandSandboxError` (nika-kernel-core) §cmd-sandbox | **wrapped-intermediate** · OS command-confinement seam error (ADR-095 Layer 6 · `io/command_sandbox.rs`). Its ONLY consumer (`nika-exec-runner` `map_sandbox_error`) flattens both variants (`Unavailable`/`Profile`) onto `ShellError::Blocked` at the runner boundary · the Shell range 050-099 wrapper carries the code |
| `JobStoreError` (nika-serve) | **transport-surface** · L4 durable job-state refusal that never crosses into the workflow/verb plane. The W06 HTTP adapter MUST map variants to bounded stable response classes and MUST NOT expose `Display`; zero NIKA registry range is owed (nika-serve crate spec §header). |
| `ApprovalHistoryError` (nika-serve) | **transport-surface** · L4 monotonic approval-history authority refusal converted immediately into `JobStoreError` at the durable server boundary. It never crosses into the workflow/verb plane; zero NIKA registry range is owed. |

## Open follow-ups (deferred-with-trigger)

1. **`AuditSinkError`** (nika-kernel-core `infra/audit.rs`) — no
   `NikaErrorCode` impl · no range allocated. The Observability range
   800-819 is reserved (v0.100 OTel adapter) and AuditSink is
   observability-adjacent. **Trigger** · first L2+ consumer that needs
   to classify audit-sink failures for retry → allocate within 800-819
   + impl. Allocating now = speculative (LOCK-031 spirit).
2. **Default-false review note** · `ShellError::Timeout` and
   `HttpError::Timeout` are `is_transient() == false` today (the
   pre-B5 semantic · preserved verbatim — B5 is a SURFACE migration,
   not a semantics change). **Trigger** · the L2 retry layer's first
   real workload decides whether shell/http timeouts are
   retry-eligible · change then, with its own test + rationale.
3. **`ManifestError`** (nika-pck-manifest `lib.rs`) — the 42nd
   admission (#306 · 2026-07-08) landed without the trait; the
   one-voice gate went RED on every subsequent push. Exempted
   `deferred-with-trigger` (allowlist row · push-train unblock
   2026-07-08 · the InferLocalError precedent). **Trigger** · the
   admitting arc's follow-up allocates a registry range + Pattern A
   impl · the allowlist row dies with it.
4. **`RecoverError`** (nika-dap `recover.rs`) — born typed at the
   crate's admission (2026-07-09 · the Gate-11 swarm's FCI-019 fix:
   `recover_events` returned a bare `String`). Exempted
   `transport-surface` (allowlist row · the LspError/McpError class):
   an L4 forensic-read error that never crosses into the
   workflow/verb plane — every consumer renders `Display` as its
   env-class message. Zero NIKA range owed.
5. **`PackError`** (nika-dap `evidence.rs`) — born typed with the A5
   evidence pack (2026-07-21 · the crate-size descent from `nika-cli`'s
   `verbs::evidence`, which never carried an error enum at all — the
   verb's env-class `VerbOutput` became a typed error at the descent).
   Exempted `transport-surface` (allowlist row · the RecoverError
   class): an L4 forensic-export error that never crosses into the
   workflow/verb plane — its ONLY consumer (the `nika trace evidence` shell)
   renders `Display` verbatim as the env-class message. Zero NIKA
   range owed.

6. **`DecodeRefusal`** (nika-dap `bounded.rs`) — the fortress decoder's
   typed total refusal (F-P1 · NEP-0012 · born typed 2026-07-24). Never
   crosses into the workflow/verb plane: `load_sidecar` flattens it to
   its reason string, `trace verify` renders the line-bound class as a
   FILE-class `VerbOutput`. Exempted `transport-surface` (allowlist row
   · the RecoverError/PackError precedent); the spec-side mint for
   fortress refusals is NEP-0012 ratification's owed.

🦋
