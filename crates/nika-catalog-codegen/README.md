# nika-catalog-codegen

Build-time codegen library that turns TOML catalog sources into Rust
source code — pure transformation, build-time-only, **zero `nika-*` deps**.

Tier-0 L0 crate. Invoked from the `build.rs` of `nika-catalog`, where it
reads the TOML inputs (providers, MCP servers, embedding models,
capability rules, …) and emits a `OUT_DIR/catalog.rs` file containing
typed `phf` perfect-hash tables. The result : `nika-catalog` carries
zero parsing cost at runtime, lookups are `O(1)`, and the data is
embedded in the binary.

This is **not a proc-macro** (no `[lib] proc-macro = true`). It is a
plain library that `build.rs` calls — simpler, faster to compile, and
fully debuggable with `cargo expand` not needed.

## Usage

From `nika-catalog/build.rs` :

```rust
use std::{env, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = PathBuf::from(env::var("OUT_DIR")?).join("catalog.rs");
    nika_catalog_codegen::emit(
        &[
            "data/providers.toml",
            "data/mcp-servers.toml",
            "data/embedding-models.toml",
        ],
        &out,
    )?;
    println!("cargo:rerun-if-changed=data/");
    Ok(())
}
```

Then `nika-catalog/src/lib.rs` does :

```rust
include!(concat!(env!("OUT_DIR"), "/catalog.rs"));
```

## Why tier-0

This crate runs in the build graph **before** any `nika-*` crate exists.
Depending on `nika-types` or `nika-error` would create a circular build
edge. Hence the L0-tier-0 status : `serde` + `toml` + `phf_codegen` +
`thiserror` only, no internal deps.

## Generated shape

The emitter produces case-insensitive `phf_map!` tables (via
`phf_shared` `unicase` feature) so that catalog lookups tolerate the
mixed-case identifiers operators type in the wild (`OpenAI` ≡ `openai`
≡ `OPENAI`).

## MSRV

Rust 1.91+.

## License

AGPL-3.0-or-later. Co-author `Nika 🦋 <nika@supernovae.studio>`.

## Related

- `crates/nika-catalog/` — the consumer, which `include!`s the emitted source
- `crates/nika-catalog-verify/` — nightly drift probe against the same TOML inputs
- `docs/architecture/crate-layer-registry.md` — L0 tier-0 contract (zero `nika-*` deps)
- `docs/adr/adr-017-foundation-publish-false.md` — foundation strategy (never published)
- `BLUEPRINT_2036.md` — `nika-catalog` is the canonical capability vocabulary shared with Olympus
