# Fix 47 Bugs — Master Plan

**Date:** 2026-03-20
**Scope:** All 47 bugs discovered by 8-agent swarm audit
**Strategy:** 7 parallel fix groups by file ownership, worktree-isolated

## Groups

### Group A — verbs.rs + extract.rs (13 bugs)

| # | Bug | Fix |
|---|-----|-----|
| 1 | `infer.system` never template-resolved | Add `template_resolve(&infer.system, ...)` before passing to provider |
| 2 | Feature-gated extract modes → lying "Unknown" error | Add `#[cfg(not(feature))]` arms returning "enable feature X" error |
| 3 | `response: binary` missing post-read size check | Add `bytes.len()` check after `response.bytes().await` |
| 4 | SSRF: no URL scheme validation in fetch | Add `http://`/`https://` scheme check before reqwest call |
| 9 | `run_fetch` zero cancellation wiring | Add `tokio::select!` with `cancel_token.cancelled()` around HTTP call |
| 10 | `llm_txt` sub-request size bypass (chunked) | Add post-read `text.len()` check after `resp.text().await` |
| 11 | `response: binary` 0-byte → cryptic CAS error | Pre-check `bytes.is_empty()`, return `{"hash": null, "size_bytes": 0}` |
| 17 | (dup of 3) binary post-download size check | Same fix as #3 |
| 28 | `MAX_VISION_IMAGE_PARTS` ignores ImageUrl | Add `ContentPart::ImageUrl` to filter |
| 31 | `llm_txt` drops original response body | Call `drop(response)` explicitly or `.text().await` before sub-requests |
| 32 | `response: full/binary` + `extract:` conflict | Add validation in `FetchParams::validate()` rejecting combination |
| 39 | Unrecognized image format → opaque error | Early-reject with NIKA error listing supported formats |
| 40 | `image_count` telemetry omits ImageUrl | Increment `image_count` in ImageUrl branch too |

### Group B — secrets/ + cli/provider + boot + TUI keys (8 bugs)

| # | Bug | Fix |
|---|-----|-----|
| 12 | `has_secret()` true for empty env vars | Change to `std::env::var(env_var).map(|v| !v.is_empty()).unwrap_or(false)` |
| 13 | `nika provider test xai` — Unknown | Add `"xai"` match arm in ProviderAction::Test |
| 14 | Boot phase 6 hardcoded, missing xAI | Iterate `KNOWN_PROVIDERS` instead of hardcoding + check `!v.is_empty()` |
| 15 | TUI `detect_state()` bypasses NIKA_KEYCHAIN_BOOT | Add NIKA_KEYCHAIN_BOOT check before NikaKeyring::get() |
| 33 | `load_*` counts empty env vars | Change `.is_ok()` to `.map(|v| !v.is_empty()).unwrap_or(false)` |
| 34 | `NikaKeyring::set/delete()` lack guard | Add `should_skip_keychain()` check |
| 35 | `MIGRATEABLE_PROVIDERS` missing xai | Add `"xai"` to the const array |
| 36 | TUI `display_name()` "Unknown" for xAI | Add `"xai" => "xAI"` match arm |

### Group C — media/ (5 bugs)

| # | Bug | Fix |
|---|-----|-----|
| 6 | CAS false-positive decompression | In `transparent_decompress`: only check marker if data was stored WITH marker (track via metadata or minimum size threshold) |
| 20 | Import 500MB vs CAS 100MB mismatch | Align `MAX_IMPORT_FILE_SIZE` to `MAX_STORE_SIZE` (100MB) |
| 21 | CasPath raw-copy leaks compressed framing | In BinarySource::CasPath handler, call `transparent_decompress` before writing artifact |
| 22 | Pipeline thumbnail `resize(w,w)` ignores height | Parse height param, use `resize_exact(w, h)` matching standalone |
| 38 | Pipeline thumbnail silently clamps | Return error like standalone tool instead of clamping |

### Group D — DAG/runtime (5 bugs)

| # | Bug | Fix |
|---|-----|-----|
| 23 | IndexedDag duplicate edges | Add `FxHashSet` dedup like `Dag::from_analyzed()` |
| 24 | for_each `{{with.alias.path}}` traversal → silent regular | Return `TaskResult::failed()` + `continue` instead of `None` |
| 25 | for_each non-array → silent regular | Same pattern: `TaskResult::failed()` + `continue` |
| 26 | `fail_fast` abort_all kills siblings | Scope abort to for_each-specific JoinSet, not shared batch JoinSet |
| 27 | `from_workflow` phantom deps → deadlock | Validate deps exist in `task_set`, return `NikaError::MissingDependency` |

### Group E — AST (6 bugs)

| # | Bug | Fix |
|---|-----|-----|
| 7 | `schema_ref` dropped in Phase 2 | Thread `schema_ref` through `AnalyzedOutput` |
| 8 | Schema file paths → Inline instead of File | Detect string values as `SchemaRef::File` in lowering |
| 41 | Invalid HTTP method → silent GET | Return analyzer error instead of defaulting |
| 42 | `output.max_retries` dropped | Add to `RawOutputConfig`, thread through pipeline |
| 43 | `response_format` dropped | Add to `RawInferAction`, thread through pipeline |
| 44 | `timeout_ms` truncation | Use ceiling division or preserve ms throughout |

### Group F — binding/ (5 bugs)

| # | Bug | Fix |
|---|-----|-----|
| 29 | `normalize_bracket_notation` corrupts literal text | Apply regex only inside `{{...}}` blocks |
| 30 | `\|length` returns bytes not chars | `s.chars().count()` instead of `s.len()` |
| 45 | `resolve_with()` template injection | Check original template, not post-Pass-1 result |
| 46 | `\|sort` lexicographic on numbers | Natural sort: numbers numerically, strings lexicographically |
| 47 | `resolve_with()` double-processes shell | Skip initial TransformExpr when shell detected |

### Group G — security/ (4 bugs)

| # | Bug | Fix |
|---|-----|-----|
| 5 | `sensitive_env_vars()` misses non-provider secrets | Add common secret env vars (AWS_SECRET_ACCESS_KEY, DATABASE_URL, etc.) |
| 16 | exec: blocklist bypass via $() | Add `$(`/backtick patterns to blocklist for shell mode |
| 18 | Policy fail-open on unparseable URLs | Change to `PolicyDecision::Block` |
| 19 | exec: env var names not validated | Validate `[A-Za-z_][A-Za-z0-9_]*` pattern |

## Execution Order

1. **Wave 1** (parallel, worktree-isolated): Groups B, C, D, E, F, G (independent files)
2. **Wave 2** (sequential after Wave 1 merge): Group A (verbs.rs — biggest, touches most code)
3. **Wave 3**: Full test suite + new bug-hunting swarm
4. **Wave 4**: E2E workflows with real LLMs

## Verification

After each group:
- `cargo test --lib` — all 6670+ tests pass
- `cargo clippy -- -D warnings` — zero warnings
- New tests for each fixed bug (regression tests)

After all groups:
- Full re-audit with 8 fresh agents
- Real workflow E2E tests with OpenAI, xAI, Gemini, Mistral
