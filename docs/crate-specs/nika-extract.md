# Crate spec — `nika-extract`

| | |
|---|---|
| Status | **SEEDED** (s17 · 2026-06-12 · admission step 12 per `crate-admission-order.md` · wired into `nika:fetch` step 13 same arc · 8 modes GREEN · full 12-gate admission at the wave close) |
| Layer | **L1.5** — pure transformation · consumed by `nika-builtin` (`nika:fetch` step 13) · above the L0 types it shares, below nothing that does I/O |
| Design | the extraction pipeline behind `nika:fetch`'s `mode:` argument — byte→structured transformation, **zero I/O · zero async · zero locks** |
| Normative source | `nika-spec stdlib/extract-modes-v0.1.md` (the 9 canonical modes + implicit `raw`) — **this doc cites, never restates** |
| LOC budget | ≤4k src (legacy reference was 1,364 LOC) · ≤1500/file · ≤100/fn |
| Crate version | tracks workspace (`0.90.0`) |
| License | `AGPL-3.0-or-later` |
| Publish | `false` — internal L1.5 |
| Error type | `ExtractError` (`thiserror` · `#[non_exhaustive]`) — `nika-builtin` maps it onto `NIKA-BUILTIN-FETCH-001` |

## §1 · Purpose

`nika:fetch` is web-**content acquisition**: HTTP (the L1 `nika-http`
effect) + extraction (this crate). The spec's 9 modes turn a response
body into the representation a downstream task actually wants —
Markdown for LLM input, structured metadata, links, feeds, sitemaps.
This crate is the PURE half: `&str` in, `serde_json::Value` out, every
mode total (never panics on arbitrary input — proptest-pinned).

## §2 · The mode set — ONE enum, three consumers

`ExtractMode` lives in **`nika-types` (L0)** — the closed vocabulary is
shared by:

1. `nika-schema` (L0) — static arg-shape rules (`mode:` literal vetting
   · conformance fixtures `stdlib/extract-modes/001..004`),
2. `nika-extract` (L1.5) — the dispatch implemented here,
3. `nika-builtin` (L1.5) — `nika:fetch` argument parsing.

One source of truth; the spec's conformance suite is the cross-repo
oracle. The set is CLOSED at v0.1 (9 + `raw` · `llm-txt` RESERVED →
rejected).

## §3 · Mode dispatch (what this crate implements)

| Mode | Module | Composes | Output (`Value`) |
|---|---|---|---|
| `markdown` | `html.rs` | `htmd` 0.5 (Apache-2.0) · skip script/style/nav/footer | String (Markdown) |
| `article` | `article.rs` | `dom_smoothie` 0.18 (MIT · Readability) → `htmd` | String (Markdown) |
| `text` | `html.rs` | `scraper` 0.27 (ISC) DOM walk · block-level `\n` | String (plain) |
| `selector` | `html.rs` | `scraper` CSS select · matches concatenated | String (HTML) |
| `metadata` | `metadata.rs` | `scraper` head walk | Object (title·description·og·twitter·canonical·lang) |
| `links` | `metadata.rs` | `scraper` + `url` join vs `base_url` | Array of absolute URL strings |
| `feed` | `feed.rs` | `feed-rs` 2.3 (MIT) | Object (title·description·link·updated·items[]) |
| `sitemap` | `sitemap.rs` | `quick-xml` 0.40 (MIT) · urlset + sitemapindex | Array of `{loc, lastmod?}` |
| `raw` | `lib.rs` | identity on the decoded body | String |
| `jq` | — | **NOT here** — `nika-builtin` composes its `jaq` runner (ONE data language, ONE engine) → `ExtractError::Unsupported` if routed here | — |

Charset note: the caller (`nika-builtin`) owns decoding — this crate
takes `&str`. Non-UTF-8 handling (spec: `raw` on non-UTF-8 is
`NIKA-BUILTIN-FETCH-001`) is the builtin's contract.

## §4 · Public API

```rust
pub use nika_types::extract::ExtractMode; // re-export (one vocabulary)

#[non_exhaustive]
pub struct ExtractOptions<'a> { pub selector: Option<&'a str>, pub base_url: Option<&'a str> }
impl ExtractOptions<'_> { pub fn new() -> Self }

#[non_exhaustive]
pub enum ExtractError { MissingArg{..}, Selector{..}, Html{..}, Feed{..}, Sitemap{..}, Unsupported{..} }

pub fn extract(body: &str, mode: ExtractMode, opts: &ExtractOptions<'_>)
    -> Result<serde_json::Value, ExtractError>;
```

## §5 · Testing strategy

Per-mode unit tests pin the spec's documented output shapes (golden
HTML/RSS/Atom/sitemap fixtures inline). Property tests (Gate 6 — this
is a parser crate): `extract()` is TOTAL over arbitrary input for every
mode (no panic · no hang); `links` never panics for arbitrary
base/href combinations. Mutation ≥90%. Legacy parity (Gate 10): golden
vectors lifted from `brouillon:tools/nika-extract` test expectations
where the spec agrees (jsonpath/llm_txt/metadata_links legacy modes are
DEAD — spec v0.1 closed set wins).

## §6 · Fences (what this crate is NOT)

- NOT an HTTP client (`nika-http` is) — zero I/O here.
- NOT the jq engine (`nika-builtin`'s `jaq` is — one data language).
- NOT charset detection (the builtin decodes; v0.1 is strict UTF-8).
- NOT media extraction (PDF/Word → `nika-media-*` · deferred stdlib v0.x).

🦋 Nika — workflow engine for AI, AGPL, SuperNovae Studio.
