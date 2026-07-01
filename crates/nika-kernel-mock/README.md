# nika-kernel-mock

Deterministic test doubles for every `nika-kernel` trait — L0.5 test
companion.

Layer **L0.5** crate (test-companion). Ships in-memory, hermetic mocks for
every kernel trait: `MockProvider`, `MockMemoryStore`, `MockFs`, `MockHttp`,
`MockShell`, `MockClock`, `MockBlobStore`, etc. Each mock is fully
deterministic (no clock drift, no network, no filesystem) — test
substitution at the kernel boundary means downstream crates inherit
hermeticity by construction, not by discipline.

## Usage

```toml
[dev-dependencies]
nika-kernel-mock = { version = "0.92", path = "../nika-kernel-mock" }
```

```rust
use nika_kernel::ai::provider::ProviderInfer;
use nika_kernel_mock::MockProvider;
use std::sync::Arc;

#[tokio::test]
async fn workflow_completes_with_canned_response() {
    let provider: Arc<dyn ProviderInfer> = Arc::new(
        MockProvider::new()
            .with_response("summary: 3 bullets ✓")
            .with_pricing_axis(0.001)
    );

    let result = my_workflow(provider).await;
    assert!(result.is_ok());
}
```

## Mocks shipped

| Trait                 | Mock                  | Notes                                    |
|-----------------------|-----------------------|------------------------------------------|
| `ProviderInfer`       | `MockProvider`        | scripted responses · cost simulation     |
| `MemoryStore`         | `MockMemoryStore`     | `BTreeMap` backing · deterministic order |
| `Fs`                  | `MockFs`              | in-memory tree · path-based              |
| `HttpClient`          | `MockHttp`            | URL → canned response                    |
| `ShellExecutor`       | `MockShell`           | cmd whitelist + stdout/stderr fixtures   |
| `Clock`               | `MockClock`           | frozen-time or virtual-tick mode         |
| `BlobStore`           | `MockBlobStore`       | content-addressed in-memory              |
| `ToolExecutor`        | `MockToolExecutor`    | tool dispatch fixtures                   |
| `WasmPluginHost`      | `MockWasmHost`        | guest-call fixtures                      |
| `ObservabilitySink`   | `MockObservability`   | recording sink for assertions            |

## Test count anchor

88+ internal lib tests verify mock invariants (deterministic order ·
fixture consumption · `Send + Sync` boundaries).

## MSRV

Rust 1.91+ (matches `[workspace.package].rust-version`).

## License

AGPL-3.0-or-later. Co-author trailer `Nika 🦋 <nika@supernovae.studio>`.

## Related

- [ADR-006 — Layered kernel + ISP traits](../../docs/adr/adr-006-layered-kernel-isp-traits.md)
- [ADR-014 — Sealed kernel traits](../../docs/adr/adr-014-sealed-kernel-traits.md) (blanket `Sealed` covers mocks)
- `nika-kernel` README — the trait surface this crate mocks
- [BLUEPRINT_2036.md](../../docs/architecture/BLUEPRINT_2036.md) — 10-year horizon
