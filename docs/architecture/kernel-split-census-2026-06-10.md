# Kernel 4-way split · trait census freeze · 2026-06-10

> **Step 1 of the locked split sequence** (`crate-layer-registry.md`
> §Reserved kernel split · ADR-006 threshold `LOC > 10k OR traits > 50`).
> Census taken at `nika-kernel` HEAD 2026-06-10 · **50 pub traits ·
> ~8.8k src LOC** — AT the trigger · the next trait admission fires it ·
> split executed NOW, before L2 verb admission adds kernel traits.
>
> This table IS the freeze — the split moves exactly these traits to
> exactly these buckets. Any trait added after this date goes directly
> to its bucket sibling.

## The partition (50 traits → 4 siblings + hub)

| Bucket (sibling crate) | Modules | Traits | Count |
|---|---|---|---|
| **nika-kernel-core** | `io/` (a11y · blob · browser · clock · fs · http · input · ocr · process · screen) + `infra/` (audit · billing · event_sink · id_gen · metrics · secret · trace) + `cancel` + `sealed` + `types` | AccessibilityTree · BlobStore · BrowserAutomation · Clock · Fs · FsList · FsMeta · FsRead · FsWrite · HttpClient · HttpGet · HttpPost · ConsentState · InputDevice · OcrEngine · ShellCancel · ShellExecutor · ShellRun · ScreenCapture · AuditSink · BillingSink · EventSink · IdGenerator · MetricsExporter · SecretResolver · TracerProvider · Sealed | **27** |
| **nika-kernel-ai** | `context` · `genai` · `memory` · `provider` · `vision` (flattened from `ai/`) | ContextCompressor · EmbeddingProvider · MemoryForget · MemoryLifecycle · MemoryRecall · MemoryRemember · MemoryStore · Provider · ProviderEmbed · ProviderInfer · ProviderMeta · ProviderStream · ProviderVision · VisionModel | **14** |
| **nika-kernel-plugin** | `sandbox` · `wasm` (flattened from `plugin/`) | Sandbox · PluginEnv · PluginFs · PluginHttp · WasmPluginHost · WasmPluginLifecycle | **6** |
| **nika-kernel-runtime** | `agent` · `tool_executor` (flattened from `runtime/`) — `checkpoint` shim corrected → hub (pure re-export of `nika_error::checkpoint` types · a facade concern) | ToolBatch · ToolExecute · ToolExecutor (+ agent/checkpoint type surfaces · 0 traits) | **3** |
| **nika-kernel** (hub) | `errors` (range registry + re-exports) + `checkpoint` shim + `lib.rs` facade + `prelude` | — (re-exports everything) | **0** |

**27 + 14 + 6 + 3 = 50.** ✓ census exact.

## Bucket-call rationale (deltas vs the pre-M1 registry sketch)

The registry sketch was written at 40 traits. The frozen census extends it ·

- The M2 computer-use trait cohort (`ScreenCapture` · `OcrEngine` ·
  `AccessibilityTree` · `InputDevice` · `ConsentState` ·
  `BrowserAutomation`) follows its `io/` home → **core**.
- `infra/` (7 observability/identity sinks) → **core** (the sketch's
  bucket list was illustrative · `io`+`infra`+root primitives form the
  base layer every other sibling depends on).
- `ContextCompressor` (`ai/context.rs`) → **ai** per the sketch's
  « Compressor » entry (the sketch's runtime « Context » refers to the
  agent checkpoint/context type surfaces · which follow `checkpoint` →
  **runtime**).
- `errors.rs` → **hub as the RANGE REGISTRY · impls DISTRIBUTED**
  (corrected at split-execution time · the original « aggregate stays
  whole in the hub » call was falsified by the **orphan rule** ·
  `NikaErrorCode` is a foreign trait from `nika-error`, so its impl
  for a type defined in a sibling MUST live in that sibling). Canon
  courant · each sibling owns the `NikaErrorCode` impls + NIKA-NNN
  constants for ITS types (core took shell 050-099 + blob 100-139 +
  http 140-189 at step 2) · the hub `errors.rs` keeps the cross-domain
  range-registry module doc + re-exports sibling constants
  (`pub use nika_kernel_core::errors::*;`) so
  `nika_kernel::errors::NIKA_050` stays a valid path.
- `Sealed` (soft seal per ADR-014 · `pub trait` · re-exported) →
  **core** · siblings bound their sealed traits via
  `nika_kernel_core::sealed::Sealed` · the soft-seal contract is
  unchanged (any crate naming it can impl it · accidental-impl
  prevention only).

## Sibling dependency DAG (downward only · enforced by check-layering)

```
nika-kernel (hub · facade + errors aggregate + prelude)
   ├── nika-kernel-ai ───────┐   (ai → core · io::screen/ocr types in vision traits)
   ├── nika-kernel-runtime ──┤   (runtime → core · none today · base types tomorrow)
   ├── nika-kernel-plugin ───┤   (plugin → core · cancel::CancelCtx)
   └── nika-kernel-core ◄────┘   (base · no internal deps)
```

All five remain **L0.5** (trait defs + companion types · async OK · no
I/O impls). `nika-kernel-mock` and every L1 effect crate keep importing
`nika_kernel::…` — the facade preserves every public path
(`pub use nika_kernel_core::{io, infra, cancel, sealed, types};` ·
`pub use nika_kernel_ai as ai;` · etc.) · **zero downstream break** ·
proven by the untouched workspace building green at each split commit.

## Step tracker

1. ✅ Census freeze (this document)
2. ✅ `nika-kernel-core` admission
3. ✅ `nika-kernel-ai` admission
4. ✅ `nika-kernel-runtime` admission
5. ✅ `nika-kernel-plugin` admission
6. ✅ Hub facade + registry/docs cascade
7. R3 narrow preludes for future L2 deps — DEFERRED until the first L2
   verb crate lands (it declares which narrow prelude it needs ·
   speculative preludes violate LOCK-031 spirit)

🦋
