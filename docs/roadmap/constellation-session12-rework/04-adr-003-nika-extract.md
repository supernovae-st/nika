# ADR-003: Extract is its own pure L2 crate, not part of `nika-verb-fetch`

**Status:** Accepted
**Date:** 2026-04-10
**Deciders:** Thibaut Melen, 4 parallel research agents
**Discovery:** This is an architectural win the original Phase 13 plan missed entirely.

## Context

`nika-engine/src/runtime/executor/extract.rs` is 1327 LOC implementing 9 HTML/data extraction modes used by the `fetch:` verb's `extract:` field:

| Mode | Purpose |
|---|---|
| `markdown` | Clean Markdown from HTML |
| `article` | Main article content (Readability algorithm) |
| `text` | Visible text, optionally filtered by CSS selector |
| `selector` | Raw HTML of matching CSS elements |
| `metadata` | OG tags, Twitter Cards, JSON-LD, SEO metadata |
| `links` | Link classification (internal/external) |
| `jsonpath` | JSONPath query on JSON responses |
| `feed` | RSS/Atom/JSON Feed parsing |
| `llm_txt` | AI content discovery (`/llms.txt`) |

The original Phase 13 plan bundled `extract.rs` into `nika-verb-fetch` alongside the HTTP logic (1399 LOC), producing a 2726-LOC verb crate. This ADR documents why that bundling is wrong and extract belongs in its own crate.

## Decision drivers

- **Single responsibility** — fetch does HTTP; extract does byte-to-structured-output transformation
- **Purity** — extract has zero I/O, zero async, zero state; it's a library function
- **Reusability** — extract could be called from other contexts (LSP, nika check previews, future CLI `nika extract` command, test fixtures)
- **Testability** — pure functions are trivially unit-testable; HTTP verbs require mocks
- **Compile time** — bundling pure code with async HTTP code slows both

## Considered options

### Option 1: Bundle `extract.rs` into `nika-verb-fetch` (the original plan)

**Pros:** one less crate, the code is already co-located in `executor/`.
**Cons:**
- `nika-verb-fetch` becomes 2700+ LOC — the largest verb crate by far (3× exec's 471)
- Mixes concerns: HTTP I/O + byte transformation in the same module
- Extract's 9-mode pure logic is drowned out by fetch's rate-limiting, cookie-jar, retry logic
- Extract cannot be reused without pulling in reqwest + cookie_store + rate_limit + robots
- `cargo test -p nika-verb-fetch --lib` compiles 2700 LOC instead of 1400 to run extract tests
- Forever couples extract to fetch's cargo features and deps

### Option 2: `nika-extract` as a separate L2 crate (this ADR)

```
tools/nika-extract/
├── Cargo.toml
├── src/
│   ├── lib.rs        — pub fn extract(mode: ExtractMode, bytes: &[u8], ctx: ExtractCtx) -> Result<ExtractOutput, ExtractError>
│   ├── error.rs      — ExtractError
│   ├── markdown.rs   — mode: markdown
│   ├── article.rs    — mode: article (Readability)
│   ├── text.rs       — mode: text
│   ├── selector.rs   — mode: selector
│   ├── metadata.rs   — mode: metadata (OG, JSON-LD, etc.)
│   ├── links.rs      — mode: links
│   ├── jsonpath.rs   — mode: jsonpath
│   ├── feed.rs       — mode: feed (RSS/Atom)
│   └── llm_txt.rs    — mode: llm_txt
```

Cargo.toml:
```toml
[dependencies]
nika-core.workspace = true     # AST: ExtractMode enum, ExtractError type
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true
# Pure extraction deps — zero I/O:
html2md.workspace = true
readability.workspace = true
scraper.workspace = true       # CSS selector parser
feed-rs.workspace = true       # RSS/Atom/JSON Feed
jsonpath-rust.workspace = true

# NOT in this Cargo.toml:
# - tokio (pure functions, no async)
# - reqwest (fetch's concern)
# - nika-engine (circular)
```

Consumer (`nika-verb-fetch`):
```rust
use nika_extract::{extract, ExtractMode};

// inside nika-verb-fetch::run:
let body_bytes = caps.http.send_streaming(request).await?.collect_limited(max_bytes).await?;
let output = nika_extract::extract(
    params.extract_mode,
    &body_bytes,
    ExtractCtx {
        base_url: &final_url,
        selector: params.selector.as_deref(),
    },
)?;
```

**Pros:**
- Single responsibility — each mode file is one page of logic
- Zero I/O / zero async — the most trivially testable code in the codebase
- Reusable from any context (tests, CLI, LSP, future previews)
- `nika-verb-fetch` shrinks from 2700+ LOC to ~600 LOC of actual HTTP logic
- `cargo test -p nika-extract --lib` compiles only pure code — under 1 second
- Extract modes can be tested independently with fixture HTML/JSON files
- Adding a 10th extract mode (hypothetically) doesn't touch `nika-verb-fetch`

**Cons:**
- One more crate in the workspace (trivial cost)
- `nika-verb-fetch` depends on `nika-extract` (normal, downward dep)

### Option 3: Keep extract in `nika-engine` and have fetch verb call it via re-export

**Pros:** no new crate, no move.
**Cons:** defeats the refactor — `nika-verb-fetch` would depend on `nika-engine` forever, breaking the diamond invariant. The whole point is to get verbs off engine.

## Decision

**Create `nika-extract` as its own L2 crate. Move `extract.rs` verbatim (1327 LOC). `nika-verb-fetch` depends on `nika-extract` and calls its pure `extract()` function.**

## Rationale

### Extract is a library, not a verb

The word "extract" in `fetch: extract: markdown` is a post-processing directive: "after receiving the bytes, run them through this transformation". The transformation has nothing to do with HTTP. It takes bytes, returns structured output. That is the definition of a library function.

Bundling it with fetch is like bundling `serde_json` with reqwest because `reqwest::Response::json()` exists. You don't. serde_json is its own crate because JSON parsing is a library concern orthogonal to HTTP.

### The discovery nobody made

The rust-architect research agent flagged this independently during the research pass, and it was the biggest architectural insight of the entire review. Quote from their report:

> "Extract (1327 LOC, 9 modes: markdown, article, text, selector, metadata, links, jsonpath, feed, llm_txt) is not a side effect — it's pure transformation of bytes into structured output. It belongs in its own crate. [...] This is the biggest hidden win in the refactor. Extract is 28% of the fetch surface area today. Separating it cleans up fetch dramatically (1399 LOC → probably ~600 LOC in nika-verb-fetch)."

The `spn-rust:rust-pro` agent confirmed it from the opposite angle when reviewing the kernel trait surface:

> "`extract.rs` has zero I/O zero async zero state. Pure functions. `nika-verb-fetch` depends on `nika-extract` as a plain library. The only API change is turning methods on `TaskExecutor` into free functions."

### Testability is dramatically better

Current: to test `extract_article()` you need to construct a `TaskExecutor` or use an integration test with a full fetch round-trip.
Target: `let output = nika_extract::extract(ExtractMode::Article, include_bytes!("fixtures/blog_post.html"), ctx)?;`

The test fixtures become the test suite. Each mode gets 5-10 fixtures. Total extract test count probably doubles vs today.

### Compile-time iteration speedup

- Current `cargo test -p nika-engine --lib extract::` compiles 148k LOC → ~90 seconds
- Target `cargo test -p nika-extract --lib` compiles ~1400 LOC → under 2 seconds

That's a **45x iteration speedup** on extract development alone.

### It unlocks the CLI `nika extract` command (Phase 15+)

Extract as a separate crate means a future `nika extract <file>` CLI command is trivial:

```bash
$ cat article.html | nika extract --mode article
# or
$ nika extract article.html --mode metadata
```

Without this split, the CLI command would drag in all of `nika-verb-fetch` and its reqwest/tokio deps.

## Consequences

### Positive

- `nika-verb-fetch` drops from 2700+ LOC to ~600 LOC
- Extract modes become trivially unit-testable
- Extract is reusable from CLI, LSP, test fixtures, future preview features
- 45x iteration speed improvement on extract development
- Future `nika extract` CLI command becomes a 10-line feature
- Diamond invariant preserved (pure L2 crate, zero engine deps)

### Negative

- One more crate in the workspace (+1 to total count, trivial)
- `extract.rs` moves, so any external references to `nika_engine::runtime::executor::extract::*` need updating (low risk — grep confirms no external consumers)

### Risks

- **Risk:** extract.rs internally imports helpers from `nika_engine::util::*` that don't travel with it. Mitigation: the Session 12 plan explicitly audits imports before the move (commit S12.F7, pre-work). Any helpers required move with the file OR get duplicated if they're trivial.
- **Risk:** extract.rs secondary fetches — one of the modes (e.g., `feed`, `llm_txt`) might call reqwest internally for sub-resources. Mitigation: audit during S12.F7 pre-work. If found, those need to take an `HttpClient` parameter or move to nika-verb-fetch. Likely NOT an issue based on current reading (extract takes bytes, doesn't fetch).

## Implementation notes

- The move happens in Session 12 (foundation phase), commit `feat(extract): create nika-extract L2 crate`.
- `extract.rs` is moved **verbatim** — no rewriting, no behavior changes. The file becomes `nika-extract/src/lib.rs` or is split into per-mode submodules if the LOC is high.
- `ExtractError` is defined locally in `nika-extract` (not from `nika-engine::NikaError`). The consumer (`nika-verb-fetch`) maps `ExtractError` to its own `FetchError` at the boundary.
- All existing `extract::*` tests move to `nika-extract/src/tests.rs` or per-mode test modules.
- The old `nika-engine/src/runtime/executor/extract.rs` file is deleted after the move verifies green (same commit).
- Workspace members updated: `+ "nika-extract"` in `tools/Cargo.toml`.

## Related decisions

- **Mega plan:** [00-mega-plan.md](00-mega-plan.md) — this extraction lands in Session 12 foundation phase
- **Session 12 foundation:** [06-session12-foundation.md](06-session12-foundation.md) — exact commit detail

## References

- **Original Phase 13 plan:** did not mention extract as a separate concern, bundled it with fetch
- **Research (rust-architect):** flagged extract as the "biggest hidden win" of the refactor
- **Research (rust-pro):** confirmed extract is pure and belongs in its own crate
- **Sans-I/O pattern (Cory Benfield):** https://sans-io.readthedocs.io/ — the philosophical precedent for separating pure logic from I/O
