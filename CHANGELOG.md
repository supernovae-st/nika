# Changelog

All notable changes to Nika are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Nika follows [forever-v0.x](ROADMAP.md) — incremental quality, no v1.0 target.

Nika Diamond is a ground-up rewrite on an orphan branch (`nika-diamond`).
Legacy main sits at v0.79.3. Diamond starts at v0.80.0.

---

## [Unreleased]

### 🔨 Refactors

- **nika-catalog Phase C migration** — migrating catalog data from hardcoded
  Rust arrays to `data/*.toml` source files, compiled at build time via
  `build.rs` + `phf_codegen`. Same zero-runtime-overhead phf maps, but the
  source of truth is now human-readable TOML. This unblocks community
  contributions to the catalog (PR a TOML file, not a Rust array).

### 🐛 Fixes

- **nika-catalog Phase A cleanup** (db0bf8e3f) — a 5-agent deep audit
  discovered 29 of our 131 MCP aliases were broken. Some pointed to
  Anthropic reference servers that were quietly deprecated ("Package no
  longer supported" on npm). Others referenced npm packages that never
  existed — Python-only tools, Go binaries, or names we'd fabricated from
  incomplete documentation. Three were community forks with zero weekly
  downloads.

  We removed all 29 and added a regression test (`removed_broken_aliases_not_present`)
  so they can't sneak back. The catalog went from 131 to 102 aliases.
  Every remaining alias now resolves to a real, installable package.

---

## [0.80.0-alpha.3] - 2026-04-13

### 🆕 Crates admitted: nika-kernel + nika-kernel-mock

The nervous system.

`nika-kernel` defines the **trait contracts for every side effect** in Nika.
It sits at L0.5 — above the pure types (error, catalog) and below the
implementations (fs, http, process, provider). Zero implementations live here.
This crate is the constitution: it says what each organ *must* do, not how.

The design follows Interface Segregation Principle to the max: ~20 fine-grained
atomic traits (`FsRead`, `FsWrite`, `HttpGet`, `ShellRun`...) grouped into ~6
super-traits of convenience (`Fs`, `HttpClient`, `ShellExecutor`, `Provider`...).
Consumers depend on exactly the surface they need — a context loader imports
`FsRead` alone, not the entire filesystem umbrella.

All async traits use `trait_variant` (Rust 1.91 native AFIT) instead of
`async_trait`. Zero boxing on the static dispatch path. The kernel carries no
tokio dependency — pure trait definitions that any async runtime can implement.

We also planted the **Cortex + agent-v2 hooks** now: `MemoryStore`,
`EmbeddingProvider`, `ToolExecutor`, `ContextCompressor`, and agent checkpoint
types. These won't be implemented until v0.95, but defining them in Phase 1
means we won't need breaking changes to `#[non_exhaustive]` structs later.
Forward compatibility bought cheaply.

`nika-kernel-mock` is the companion: deterministic mocks for every kernel trait
(`MockClock`, `InMemoryFs`, `MockHttp`, `MockShell`, `MockProvider`...).
Test hermeticity from day one — no test in Nika will ever touch a real
filesystem, a real network, or a real LLM provider.

| Metric | nika-kernel | nika-kernel-mock |
|--------|-------------|------------------|
| LOC | 3,369 | 1,731 |
| Tests | 99 | 88 |
| Mutation killed | 100% | 95.7% |
| Clippy warnings | 0 | 0 |
| Unwraps in src/ | 0 | 0 |

### Key decisions

- **Clock is SYNC, everything else ASYNC** — YAGNI on network time. Hot paths
  stay simple.
- **`BTreeMap` over `HashMap`** — deterministic iteration order, no hasher
  dependency. Tests are reproducible.
- **Cancel as `fn` param, not in struct** — keeps `ShellCommand` free of
  tokio-util. The kernel stays runtime-agnostic.
- **Provider = Infer + Stream + Meta** — all providers MUST stream (even mock).
  Embed and Vision are opt-in traits.
- **Errors per subsystem** — `ProviderError`, `ShellError`, `ToolExecError`,
  `MemoryError`. No god-enum.

All 12 gates passed. Commit `ef8804371`. 🦋

---

## [0.80.0-alpha.2] - 2026-04-13

### 🆕 Crate admitted: nika-catalog

The memory.

`nika-catalog` is Nika's static knowledge of the world: every LLM provider it
can talk to, every MCP server it knows how to install, every builtin tool it
ships, every pipe transform it supports, and the pricing of every model it's
seen.

The catalog is compiled into the binary at build time. No runtime I/O, no
config files, no network calls. You ask "do you know `anthropic`?" and the
answer comes back in O(1) via a [perfect hash function](https://en.wikipedia.org/wiki/Perfect_hash_function).

Why this matters: when a user writes `provider: claude` in their YAML, the
engine resolves the alias → canonical provider → model → capabilities → pricing
in a chain of zero-allocation lookups. No guessing, no fuzzy matching, no
"did you mean?" The catalog is the ground truth.

The lookup strategy is hybrid by design:
- **phf + unicase** for case-insensitive lookups (providers, MCP aliases) —
  because users write `Claude`, `claude`, `CLAUDE` and they all mean Anthropic.
- **Sorted arrays + binary_search** for case-sensitive lookups (builtins,
  transforms) — because `nika:read` and `nika:Read` are different things
  (actually `nika:Read` doesn't exist, and the catalog should say so clearly).

At admission: 16 providers, 113 MCP aliases, 63 builtins, 65 transforms,
61 model pricing entries. All from a single `cargo build`.

| Metric | Value |
|--------|-------|
| LOC | 2,235 |
| Tests | 85 |
| Mutation killed | 94.7% |
| Clippy warnings | 0 |
| Unwraps in src/ | 0 |

All 12 gates passed. Commit `55a451695`. 🦋

---

## [0.80.0-alpha.1] - 2026-04-13

### 🆕 Crate admitted: nika-error

The DNA.

Every error in Nika carries a code. `NIKA-001` means schema validation failed.
`NIKA-053` means a blocked command was attempted. `NIKA-382` means a canary
token leaked (prompt injection detected). There are hundreds of these codes,
and every single one must roundtrip through Display, parse back from a string,
serialize to JSON, and match the exact same format across every provider, every
verb, every transport layer.

`nika-error` is the crate that makes this possible. It defines:

- **`NikaErrorCode`** — a trait that every per-crate error enum must implement.
  This is the contract: if you want to be a Nika error, you carry a code, a
  severity, a category, and you format yourself as `"NIKA-XXX: message"`.
- **`NikaError`** — a `Box<dyn NikaErrorCode>` wrapper. The unified error type
  that flows through `?` propagation across the entire codebase.
- **`NikaCode`** — the code itself. Dual format: Display gives you `"NIKA-140"`,
  serde gives you `{"num":140,"category":"ast","severity":"error","slug":"ast-analysis-failure"}`.
- **`CoreError`** — cross-cutting errors that don't belong to any specific crate
  (Validation, NotFound, Unsupported, Internal).

This is the L0 anchor. Zero `nika-*` dependencies. Reachable from every crate
in the workspace. The first cell of the organism.

It also resolves **shadow zone 6** from the pre-launch audit: every admitted
`NIKA-XXX` now ships with a Display parity golden test against the legacy
format. No silent drift.

| Metric | Value |
|--------|-------|
| LOC | 1,013 |
| Tests | 44 |
| Mutation killed | 100% |
| Clippy warnings | 0 |
| Unwraps in src/ | 0 |

All 12 gates passed. Commit `42909b1c7`. 🦋

---

## [0.80.0-alpha.0] - 2026-04-13

### The beginning

Orphan branch `nika-diamond` created from scratch. No code inherited from main.
Clean slate, edition 2024, Rust 1.91.

From the start, the workspace enforces:
- `clippy::unwrap_used = "deny"` — zero unwraps, everywhere, always.
- `clippy::panic = "deny"` — if it can panic, it doesn't compile.
- `clippy::expect_used = "warn"` — we'll get there.

32 legacy crate directories excluded via `.gitignore` — they exist on disk
(the orphan branch inherits the working tree) but cargo ignores them. We read
legacy code via `git show main:path/to/file.rs` when we need guidance, but
nothing is copied verbatim. Every line is rewritten.

The organism's skeleton is in place. Now it grows. 🦋

---

[Unreleased]: https://github.com/supernovae-st/nika/compare/v0.80.0-alpha.3...HEAD
[0.80.0-alpha.3]: https://github.com/supernovae-st/nika/compare/v0.80.0-alpha.2...v0.80.0-alpha.3
[0.80.0-alpha.2]: https://github.com/supernovae-st/nika/compare/v0.80.0-alpha.1...v0.80.0-alpha.2
[0.80.0-alpha.1]: https://github.com/supernovae-st/nika/compare/v0.80.0-alpha.0...v0.80.0-alpha.1
[0.80.0-alpha.0]: https://github.com/supernovae-st/nika/commits/v0.80.0-alpha.0
