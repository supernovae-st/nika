# nika-catalog-verify

Online drift probe for `nika-catalog` — verifies that pinned npm / PyPI / OCI
packages and remote MCP endpoints still resolve, and reports drift.

This is a **binary, not a library**. It is an L4 dev-tool that the
nightly job (or a curious operator) runs against the catalog Rust types
generated at build-time by `nika-catalog-codegen`. It is not consumed by
any other workspace crate.

## Usage

Run via cargo from the engine root :

```
cargo run -p nika-catalog-verify -- --help
cargo run -p nika-catalog-verify -- --concurrency 16 --timeout 30s
cargo run -p nika-catalog-verify --release -- --format json > drift.json
```

Or, after a release build, invoke the bin directly :

```
./target/release/nika-catalog-verify --filter npm
```

Exit codes :

| code | meaning |
|---|---|
| `0` | every probe matched the catalog |
| `1` | one or more probes drifted (version moved, package yanked, endpoint changed) |
| `2` | network / config error before any probe ran |

`tracing-subscriber` is wired with `EnvFilter`, so set
`RUST_LOG=nika_catalog_verify=debug` for verbose traces.

## What it probes

- **npm** registry · resolves `package@version` against `registry.npmjs.org`
- **PyPI** registry · same shape via JSON API
- **OCI** registries · manifest HEAD against `ghcr.io` / `docker.io` / vendor
- **MCP** endpoints · reachability + capability handshake

## Why a binary

Probing the network is L4 work (I/O, async, retries, concurrency). The
catalog itself (`nika-catalog`) stays L0 — a pure compiled Rust source
emitted from TOML by `nika-catalog-codegen`. Splitting probe from data
keeps the foundation layer hermetic.

## MSRV

Rust 1.91+.

## License

AGPL-3.0-or-later. Co-author `Nika 🦋 <nika@supernovae.studio>`.

## Related

- `crates/nika-catalog/` — the static data this binary verifies against
- `crates/nika-catalog-codegen/` — build-time codegen feeding `nika-catalog`
- `docs/architecture/crate-layer-registry.md` — L4 layer contract (allowed I/O)
- `docs/adr/adr-017-foundation-publish-false.md` — why this binary is never published to crates.io
- `BLUEPRINT_2036.md` — `init + lints + catalog-verify` collapse to `nika-cli` subcommands target
