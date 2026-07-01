# Crate spec — `nika-extract`

| | |
|---|---|
| Status | **ADMITTED** (all 12 gates · s17 · seeded 2026-06-12 · wired into `nika:fetch` step 13 · 9 modes · admitted 2026-06-21 · §7 gate table) |
| Layer | **L1.5** — pure transformation · consumed by `nika-builtin` (`nika:fetch` step 13) · above the L0 types it shares, below nothing that does I/O |
| Design | the extraction pipeline behind `nika:fetch`'s `mode:` argument — byte→structured transformation, **zero I/O · zero async · zero locks** |
| Normative source | `nika-spec stdlib/extract-modes-v0.1.md` (the 9 canonical modes + implicit `raw`) — **this doc cites, never restates** |
| LOC budget | ≤4k src (legacy reference was 1,364 LOC) · ≤1500/file · ≤100/fn |
| Crate version | tracks workspace |
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

## §7 · The 12 gates (admission · 2026-06-21)

| Gate | Status |
|---|---|
| 1 SPEC | ✅ this file |
| 2 TDD | ✅ per-mode unit tests + the adversarial suite |
| 3 IMPL | ✅ 9 modes · zero `.unwrap()` in `src/` · `#[forbid(unsafe_code)]` |
| 4 CLIPPY | ✅ 0 warnings (`--all-targets -D warnings`) |
| 5 MUTATION | ✅ 93.2% killed (373/400 viable · timeout-as-missed floor · 98% counting the depth-guard hang-kills) · residual 8 documented (3rd-party-gated cascade · author-documented sitemap no-op · niche boundaries) |
| 6 PROPERTY | ✅ `extract_never_panics` (totality · all modes · arbitrary input) + `links_total_over_hostile_bases` |
| 7 BENCHMARKS | N/A — the extraction LOGIC is not the hot path; the html5ever/scraper/quick-xml parse cost dominates and is the upstream libs' concern |
| 8 DOCS | ✅ `cargo doc --no-deps --document-private-items` 0 warnings |
| 9 CANARY E2E | ✅ `tests/adversarial.rs` (24 hostile-input cases · billion-laughs · XXE · DoS-bounding · bypass closure) + transitive coverage via the `nika:fetch` extract step in `nika-cli`'s e2e_pipeline |
| 10 PARITY LEGACY | N/A — the v0.79 brouillon legacy modes (jsonpath/llm_txt/metadata_links) are DEAD by design (spec v0.1 closed set); the kept modes are a CRAFT rewrite (ADR-001), not a line-by-line port |
| 11 REVIEW SWARM | ✅ 3-agent swarm (Nika-conventions · bug/logic · adversarial-refuter) · refuter SURVIVED (totality holds) · P1 og-URL absolutization fixed · 1 finding verify-before-fix rejected as W3C-correct |
| 12 ATOMIC | ✅ this admission commit |

🦋 Nika — workflow engine for AI, AGPL, SuperNovae Studio.
