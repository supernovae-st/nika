# nika-kernel

Sealed trait contracts for every side effect in the Nika diamond — the L0.5
abstraction boundary.

Layer **L0.5** crate. Defines ~20 atomic ISP traits across 5 internal module
groups (`io` · `ai` · `runtime` · `plugin` · `infra`) — every filesystem
read, HTTP call, LLM inference, memory recall, tool invocation, WASM host
op, observability emit. **Zero implementations live here.** L1 effect
crates implement the contracts; `nika-kernel-mock` provides deterministic
test doubles. All traits are `private::Sealed` per ADR-014 — external
crates cannot impl directly, route through workspace adapters or future
`pck` plugins.

## Architecture

```text
Clock (sync + async sleep)
Fs            = FsRead + FsWrite + FsMeta + FsList
HttpClient    = HttpGet + HttpPost
ShellExecutor = ShellRun + ShellCancel
BlobStore (atomic, no split)
Provider      = ProviderInfer + ProviderStream + ProviderMeta
              + opt-in: ProviderEmbed · ProviderVision
MemoryStore   = MemoryRemember + MemoryRecall + MemoryForget
              + EmbeddingProvider
ToolExecutor  = ToolExecute + ToolBatch
WasmPluginHost · ObservabilitySink
```

Every async trait uses `#[trait_variant::make(Send)]` (per ADR-006) — Rust
1.91 native AFIT on the static-dispatch hot path, `Dyn`-suffixed companion
trait for object safety when boxing is needed.

## Usage

```toml
[dependencies]
nika-kernel = { version = "0.80", path = "../nika-kernel" }
```

```rust
use nika_kernel::ai::provider::{ProviderInfer, InferRequest, InferResponse};
use nika_kernel::ai::memory::MemoryRecall;
use std::sync::Arc;

async fn ask<P: ProviderInfer + Send>(
    provider: Arc<P>,
    memory: Arc<dyn MemoryRecall>,
    query: &str,
) -> Result<InferResponse, nika_error::NikaError> {
    // recall context · invoke provider · return response
    todo!()
}
```

## Forward-compat hooks (ADR-007)

Pre-planted at Phase 0 for v0.95 Cortex + v0.100 agent-v2 ·
- `MemoryStore` + `EmbeddingProvider` — Cortex
- `ToolExecutor` — parallel tool calling
- `WasmPluginHost` — WASM plugins
- `ObservabilitySink` — tracing + metrics

`InferRequest` carries reserved `Option<MemoryDirective>` and `Option<BudgetDirective>` — new fields land additively per `#[non_exhaustive]` discipline.

## MSRV

Rust 1.91+ (matches `[workspace.package].rust-version`).

## License

AGPL-3.0-or-later. Co-author trailer `Nika 🦋 <nika@supernovae.studio>`.

## Related

- [ADR-006 — Layered kernel + ISP traits](../../docs/adr/adr-006-layered-kernel-isp-traits.md)
- [ADR-014 — Sealed kernel traits](../../docs/adr/adr-014-sealed-kernel-traits.md)
- [ADR-039 — Streaming MemoryRecall (W4)](../../docs/adr/adr-039-streaming-memory-recall.md)
- [BLUEPRINT_2036.md](../../docs/architecture/BLUEPRINT_2036.md) — 10-year horizon
