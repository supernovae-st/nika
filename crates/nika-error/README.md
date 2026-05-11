# nika-error

Canonical error taxonomy for the Nika diamond — `NIKA-XXX` codes, sealed
enum, `miette` diagnostics.

Layer **L0** crate. Ships the workspace-wide `NikaError` sealed enum (90+
variants), the `NIKA-XXX` code taxonomy (per ADR-005 hierarchy), and the
`NikaErrorCode` trait that every crate-local error implements for category
+ retryability + display routing. Zero I/O, zero async — pure types plus
`thiserror` + `miette` integration.

## Usage

```toml
[dependencies]
nika-error = { version = "0.80", path = "../nika-error" }
```

```rust
use nika_error::{NikaError, NikaErrorCode};

fn ingest(payload: &str) -> Result<(), NikaError> {
    if payload.is_empty() {
        return Err(NikaError::ValidationFailed {
            field: "payload".into(),
            reason: "empty body".into(),
        });
    }
    Ok(())
}

// Every variant carries a stable NIKA-XXX code + category + retry hint.
let err = NikaError::MemoryStorage { /* ... */ };
assert_eq!(err.code(), "NIKA-604");
assert!(!err.is_transient());
```

## Code taxonomy

NIKA-XXX codes are organized per `Category` enum (ADR-005) ·

| Range       | Category      | Examples                          |
|-------------|---------------|-----------------------------------|
| `100..=199` | Schema        | `SchemaInvalid` · `EnvelopeMissing` |
| `200..=299` | Binding       | `TemplateUnresolved`              |
| `300..=399` | Shield        | `CapabilityDenied` · `TrustViolation` |
| `400..=499` | Catalog       | `ModelUnknown` · `BuiltinMissing` |
| `500..=599` | Provider      | `ProviderRateLimited`             |
| `600..=649` | Memory        | `MemoryUnavailable` · `EmbeddingFailed` |
| `620..=629` | Memory · BM25 | reserved for `nika-bm25` admission W3 |
| `650..=699` | Runtime / DAG | `RunDepthExceeded` · `RunCycleDetected` |

## Features

- `default = ["serde"]` — derive `Serialize`/`Deserialize` on every variant
- `serde` — opt out via `default-features = false` for `no_std`-adjacent embed

## MSRV

Rust 1.91+ (matches `[workspace.package].rust-version`).

## License

AGPL-3.0-or-later. Co-author trailer `Nika 🦋 <nika@supernovae.studio>`.

## Related

- [ADR-005 — Error hierarchy](../../docs/adr/adr-005-error-hierarchy.md)
- [ADR-007 — Forward-compat invariants](../../docs/adr/adr-007-forward-compat-invariants.md) (`#[non_exhaustive]` everywhere)
- [BLUEPRINT_2036.md](../../docs/architecture/BLUEPRINT_2036.md) — 10-year horizon
