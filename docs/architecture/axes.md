# ISP Capability Axes — Cross-Crate Matrix

Cross-reference of the **12 capability axes** (effect channels) × **admitted crates** × **status**. Makes the ISP decomposition
visible at a glance and surfaces gaps before they become wrong abstractions. Maintained by reading the actual crate sources — not a plan.

## Status legend

- **🟢 shipped** — trait declared, impl lives in workspace, tests pass
- **🟡 reserved** — trait/types declared, no impl yet (forward-compat seam)
- **⬜ not yet** — axis identified but no reservation in-tree
- **—** — axis does not apply to the crate

## Axis map (12)

| Axis | Short description | Primary location |
|---|---|---|
| **clock** | Wall + monotonic time | `nika-kernel::io::clock` |
| **fs-read** | Read-only filesystem | `nika-kernel::io::fs::FsRead` |
| **fs-write** | Mutating filesystem | `nika-kernel::io::fs::FsWrite` |
| **fs-meta+list** | Stat + directory walk | `nika-kernel::io::fs::{FsMeta, FsList}` |
| **http** | Out-of-process HTTP | `nika-kernel::io::http::{HttpGet, HttpPost}` |
| **process** | Shell / child process | `nika-kernel::io::process::{ShellRun, ShellCancel}` |
| **blob** | Content-addressable bytes | `nika-kernel::io::blob::BlobStore` |
| **secret** | Secret reference resolution | `nika-kernel::infra::secret::SecretResolver` |
| **billing** | Cost metering (never sampled) | `nika-kernel::infra::billing::BillingSink` |
| **event** | Telemetry events (sampleable) | `nika-kernel::infra::event_sink::EventSink` |
| **metrics** | Counters / gauges / histograms | `nika-kernel::infra::metrics` |
| **trace** | Distributed tracing (W3C / OTLP) | `nika-kernel::infra::trace::TracerProvider` |
| **audit** | Compliance-grade append-only log | `nika-kernel::infra::audit::AuditSink` |
| **memory** | Store / recall / forget / lifecycle | `nika-kernel::ai::memory::{MemoryStore, MemoryLifecycle}` |
| **embedding** | Dense vector generation | `nika-kernel::ai::memory::EmbeddingProvider` |
| **tool-exec** | Builtin + MCP tool dispatch | `nika-kernel::runtime::tool_executor::ToolExecutor` |
| **sandbox** | Capability gates | `nika-kernel::plugin::sandbox::Sandbox` |
| **wasm-host** | WASM plugin execution | `nika-kernel::plugin::wasm::WasmPluginHost` |

## Crate × Axis matrix

Columns left-to-right mirror the layer stack. Rows list the 7 admitted
crates + 1 WIP (nika-schema) in workspace order.

| Axis / Crate            | nika-types | nika-error | nika-catalog | nika-catalog-verify | nika-schema | nika-kernel | nika-kernel-mock |
|-------------------------|:----------:|:----------:|:------------:|:-------------------:|:-----------:|:-----------:|:----------------:|
| Layer                   | L0         | L0         | L0           | L4                  | L0 (wip)    | L0.5        | L0.5             |
| clock                   | —          | —          | —            | —                   | —           | 🟢 trait    | 🟢 mock          |
| fs-read                 | —          | —          | —            | 🟢 uses             | —           | 🟢 trait    | 🟢 mock          |
| fs-write                | —          | —          | —            | —                   | —           | 🟢 trait    | 🟢 mock          |
| fs-meta+list            | —          | —          | —            | —                   | —           | 🟢 trait    | 🟢 mock          |
| http                    | —          | —          | —            | —                   | —           | 🟢 trait    | 🟢 mock          |
| process                 | —          | —          | —            | —                   | —           | 🟢 trait    | 🟢 mock          |
| blob                    | —          | —          | —            | —                   | —           | 🟢 trait    | 🟢 mock          |
| secret                  | —          | —          | —            | —                   | —           | 🟢 trait    | 🟢 mock          |
| billing                 | 🟢 `Cost`, `TokenUsage` | 🟢 re-export | —           | —                 | —           | 🟢 `BillingSink` | 🟢 null   |
| event                   | 🟢 `Baggage`, `EventId` | 🟢 re-export | —           | —                 | —           | 🟢 `EventSink`   | 🟢 null   |
| metrics                 | —          | —          | —            | —                   | —           | 🟢 trait    | 🟢 null          |
| trace                   | 🟢 `TraceId`, `SpanId` | 🟢 re-export | —       | —                   | —           | 🟢 `TracerProvider` + `parent_span_id` (4B) | 🟢 null |
| audit                   | —          | —          | —            | —                   | —           | 🟢 `AuditSink` (Q12) | 🟢 null |
| memory: remember/recall/forget | 🟢 IDs+frames | 🟢 re-export | —    | —                   | —           | 🟢 3 traits | 🟢 null          |
| memory: lifecycle (consolidate+prune) | — | — | —       | —                   | —           | 🟡 reserved 4A-R5 | 🟡 default no-op |
| embedding generation    | 🟢 `EmbeddingSpec` (4A-R1) | 🟢 re-export | — | —               | —           | 🟢 `EmbeddingProvider` | 🟢 null |
| tool-exec               | —          | —          | —            | —                   | —           | 🟢 trait    | 🟢 mock          |
| sandbox                 | —          | —          | —            | —                   | —           | 🟢 trait    | 🟢 mock          |
| wasm-host               | —          | —          | —            | —                   | —           | 🟡 traits + `OutOfFuel`/`Trap`/`PluginCallContext` (4A-R4) | 🟡 null |
| timestamps (`Timestamp` / `WallDuration`) | 🟢 types (Q9/4B-#3) | 🟢 re-export | — | —   | —           | 🟡 retrofit pending | —  |

## Read-the-code commands

```bash
# Every trait in nika-kernel's src
grep -rn '^pub trait' crates/nika-kernel/src/

# Every #[trait_variant::make(...)] declaration
grep -rn 'trait_variant::make' crates/nika-kernel/src/

# Every impl of every kernel trait in the mock
grep -rn 'impl .* for ' crates/nika-kernel-mock/src/

# Which axis exports pass through nika-error facade
grep -rn 'pub use nika_types::' crates/nika-error/src/lib.rs

# Regenerate the full public API surface per crate
cargo public-api -p nika-kernel --all-features --omit auto-trait-impls
```

## What this matrix is for

1. **Forward-compat review**: when adding a new axis (e.g. v0.95 adds
   `rag-retriever`), we visually check every L0/L0.5 crate to decide
   which types belong where. Avoids sprinkling a new trait across
   three layers.
2. **Gate 12 (FCI) audit**: every shipped axis must have either a
   workspace impl or a documented reserved status. Yellow rows
   force a "will land in vN" answer in the ADR.
3. **ISP discipline**: a crate row with ≥ 3 shipped axes is a code
   smell — it likely violates ISP and should split. None today;
   check again before every phase merge.
4. **Olympus graph layer**: exported as a structural input so the
   `/graph/architecture` view can render effect-channel edges
   (who consumes which axis) without re-reading source at display
   time.

## Maintenance policy

* **Update on every admission**: a new crate row gets added, each
  axis filled 🟢/🟡/⬜.
* **Update on every reservation**: moving a cell from ⬜ to 🟡 requires
  the reservation commit to also touch this file (Gate 12 rule).
* **Update on every impl**: moving a cell from 🟡 to 🟢 requires a
  test asserting the trait is implemented (compile-time guard like
  `fn _check<T: MyTrait>()`).
* **No stale yellows**: a reservation that sits yellow for > 3 minor
  versions without a consumer becomes a design smell — review in the
  canonical architecture plan.
* **Regenerate mechanically**: the matrix should be derivable from
  source + a small script; if it drifts more than twice, write the
  script.

See also:

* `docs/architecture/forward-compat-invariants.md` — the 8 patterns
* `docs/architecture/l0-l05-architecture-decisions.md` — Q1-Q13 locks
* `docs/architecture/crate-layer-registry.md` — layer rules
* `scripts/hygiene/check-layering.sh` — automated layer check
* `scripts/hygiene/check-layer-deps.sh` — vector 33 banned deps
