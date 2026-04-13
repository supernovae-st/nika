# nika-core

Lightweight AST and analysis core for Nika workflows. Zero runtime dependencies.

## Purpose

Contains the protocol-agnostic parts of Nika — everything needed for parsing, validation, and analysis without pulling in tokio, reqwest, or any heavy runtime.

## Key Modules

| Module | Purpose |
|--------|---------|
| `ast/raw/` | Phase 1: YAML → RawWorkflow (parser.rs) |
| `ast/analyzed/` | Phase 2: Validated, resolved AST |
| `ast/analyzer/` | Validation + transformation + error suggestions |
| `binding/` | Transform catalog (27 pipe transforms) |
| `core/` | Provider definitions, model catalog, MCP aliases |

## Does NOT contain

- Runtime execution (tokio, reqwest, rig-core)
- Media pipeline (image, zstd, c2pa)
- MCP client (rmcp)

## License

AGPL-3.0-or-later
