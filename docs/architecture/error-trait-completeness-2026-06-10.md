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

## Documented exemptions (not gaps)

| Enum | Why exempt |
|---|---|
| `CodegenError` (nika-catalog-codegen) | BUILD-TIME tool error (build.rs generator) · never crosses a runtime/verb boundary · thiserror Display suffices · zero NIKA range owed |
| `GoDurationError` · `ExprError` (nika-schema) | internal intermediate errors · wrapped into `SchemaError` before crossing the crate boundary · the wrapper carries the code |
| `ToolErrorPolicy` · `OnError` · `ErrorCategory` | NOT error types (policy/config enums matching the `*Error` name pattern) |
| `TestError`/`OtherError` (traits.rs tests) | test fixtures |
| `ExtractError` (nika-extract) | pure L1.5 transformation error · its only consumer (`nika-builtin` fetch) flattens every variant onto the spec-form `NIKA-BUILTIN-FETCH-001` string at the dispatcher boundary (crate spec) · the builtin plane carries the code |

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

🦋
